use super::*;

/// Builds a plain text paragraph — the single source shared by measure and draw.
///
/// Builds a skia Paragraph from the TextStyle parameters (max_lines, ellipsis, justify, weight,
/// slant) and picks the layout width from soft_wrap. Both the measure path (node.rs) and the draw
/// fallback (render.rs) call it, so the two cannot drift apart in style.
pub(crate) fn build_plain_paragraph(
    content: &str,
    font_size: f32,
    color: &crate::graphics::Color,
    font_weight: crate::text::FontWeight,
    font_style: crate::text::FontSlant,
    max_lines: usize,
    align: crate::text::TextAlign,
    overflow: crate::text::TextOverflow,
    soft_wrap: bool,
    letter_spacing: f32,
    line_height: Option<f32>,
    underline: bool,
    line_through: bool,
    font_family: Option<&str>,
    paint: Option<TextPaintSpec>,
    max_width: f32,
) -> crate::text::Paragraph {
    use skia_safe::textlayout::ParagraphStyle;

    let mut para_style = ParagraphStyle::new();

    // Compose's `TextDelegate.layoutText` (TextDelegate.kt:160-181): the constraint width matters when
    // the text WRAPS or when it ELLIPSIZES, and a nowrap ellipsis is a ONE-LINE ellipsis — native text
    // layout cannot produce several of them, so Compose overwrites maxLines with 1 there. Laying a
    // nowrap ellipsis out at an infinite width (what this did) makes Skia believe the line fits and no
    // ellipsis is ever produced.
    let width_matters = soft_wrap || overflow.is_ellipsis();
    let effective_max_lines = if !soft_wrap && overflow.is_ellipsis() {
        1
    } else {
        max_lines
    };
    if effective_max_lines < usize::MAX {
        para_style.set_max_lines(effective_max_lines);
    }

    // ellipsis overflow: show an ellipsis when the text does not fit
    // Only the END ellipsis is a Skia flag. Start/middle ellipsis truncate the STRING before layout
    // (see `truncate_for_ellipsis`) — `set_ellipsis` cannot move the marker.
    if overflow == crate::text::TextOverflow::Ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    // Alignment. When the WIDTH MATTERS the paragraph knows the box, so Skia can align every line
    // inside it — which is the only way a wrapped block's short lines sit under the right edge
    // (Compose's `textAlign` is per line). When it does not (a nowrap, non-ellipsizing paragraph
    // laid out at an infinite width) there is no box to align in, and the paint origin is shifted
    // instead: see the text branch in `render::pass`.
    if width_matters {
        match align {
            crate::text::TextAlign::Center => { para_style.set_text_align(skia_safe::textlayout::TextAlign::Center); }
            crate::text::TextAlign::Right => { para_style.set_text_align(skia_safe::textlayout::TextAlign::Right); }
            crate::text::TextAlign::Justify => { para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify); }
            // `Start`/`End` never reach a built paragraph (`Text::build` resolves them), but a
            // caller can hand one straight to the builder, so treat them as the physical default.
            crate::text::TextAlign::Left
            | crate::text::TextAlign::Start
            | crate::text::TextAlign::End => {}
        }
    }

    // A nowrap paragraph is laid out at an infinite width so nothing wraps — unless it ellipsizes, in
    // which case the box width is what the ellipsis truncates against (see the width_matters note
    // above). The f32::MAX sentinel some scroll paths use IS a bounded width, so clamp the infinity.
    let layout_width = if width_matters {
        max_width.min(f32::MAX)
    } else {
        f32::MAX
    };

    let mut text_style = skia_safe::textlayout::TextStyle::new();
    text_style.set_font_size(font_size);
    // IMPORTANT: set the text colour (Skia's TextStyle defaults to white, which is invisible on a
    // white background)
    text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
    // Letter spacing (Compose's TextStyle.letterSpacing, in logical pixels)
    if letter_spacing != 0.0 {
        text_style.set_letter_spacing(letter_spacing);
    }
    // Line height (Compose's TextStyle.lineHeight, a fixed pixel value; skia speaks in multipliers,
    // so set_height(multiplier) plus set_height_override(true) — only the override makes it stick,
    // matching the RichText path, otherwise the font's default height wins)
    if let Some(lh) = line_height {
        if lh > 0.0 && font_size > 0.0 {
            text_style.set_height(lh / font_size);
            text_style.set_height_override(true);
            // Compose's default `LineHeightStyle(alignment = Center, trim = None)` splits the extra
            // leading evenly above and below the glyphs (M3's `TypographyTokens.kt:329-330`).
            // Skia's plain `set_height` puts ALL of it above the text — measured at 16 sp with a
            // 24 dp line height, the baseline landed at 19.47 in a 24 dp box; `half_leading`
            // centres it (see D-2 in docs/text-review.md).
            text_style.set_half_leading(true);
        }
    }
    // A font family, when the caller named one (Compose's `fontFamily`).
    if let Some(family) = font_family {
        text_style.set_font_families(&[family]);
    }
    // Compose's `textDecoration` — the plain leaf draws it now (the flags used to be RichText-only).
    if underline || line_through {
        use skia_safe::textlayout::TextDecoration as SkDecoration;
        let mut decoration = SkDecoration::NO_DECORATION;
        if underline {
            decoration |= SkDecoration::UNDERLINE;
        }
        if line_through {
            decoration |= SkDecoration::LINE_THROUGH;
        }
        text_style.set_decoration_type(decoration);
    }
    // Weight and slant
    if font_weight != crate::text::FontWeight::NORMAL || font_style != crate::text::FontSlant::Upright {
        use skia_safe::FontStyle;
        use crate::text::FontSlant;
        let slant = match font_style {
            FontSlant::Upright => skia_safe::font_style::Slant::Upright,
            FontSlant::Italic => skia_safe::font_style::Slant::Italic,
            FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
        };
        text_style.set_font_style(FontStyle::new(font_weight.value().into(), 5.into(), slant));
    }
    // Compose's `TextStyle.shadow`: Skia draws it behind the glyphs and it does not take part in
    // metrics (a shadow never grows the measured box — measured in the same run that added it).
    if let Some(shadow) = paint.as_ref().and_then(|p| p.shadow) {
        text_style.add_shadow(skia_safe::textlayout::TextShadow::new(
            skia_safe::Color::from_argb(shadow.color.a, shadow.color.r, shadow.color.g, shadow.color.b),
            (shadow.offset.0, shadow.offset.1),
            shadow.blur_sigma(),
        ));
    }
    // Compose's `TextStyle.fontFeatureSettings`: OpenType features, applied in order.
    if let Some(settings) = paint.as_ref().and_then(|p| p.font_feature_settings.clone()) {
        for (tag, value) in crate::text::style::parse_font_features(settings.as_str()) {
            text_style.add_font_feature(tag.as_str(), value);
        }
    }
    // Compose's `TextStyle.drawStyle` and `TextStyle.brush`: both are a FOREGROUND PAINT on the Skia
    // style, which replaces the flat colour the paragraph would otherwise paint with.
    if let Some(paint_spec) = paint.as_ref() {
        let brush = paint_spec.brush.as_ref();
        let stroke = match paint_spec.draw_style {
            Some(crate::text::TextDrawStyle::Stroke { width }) if width > 0.0 => Some(width),
            _ => None,
        };
        if brush.is_some() || stroke.is_some() {
            let mut glyph_paint = skia_safe::Paint::default();
            glyph_paint.set_anti_alias(true);
            if let Some(brush) = brush {
                use skia_safe::gradient::{Gradient, Interpolation};
                use skia_safe::gradient::Colors;
                let colors: Vec<skia_safe::Color4f> = brush
                    .colors
                    .iter()
                    .map(|c| skia_safe::Color4f::new(
                        c.r as f32 / 255.0,
                        c.g as f32 / 255.0,
                        c.b as f32 / 255.0,
                        c.a as f32 / 255.0,
                    ))
                    .collect();
                if colors.len() >= 2 {
                    let gradient_colors = Colors::new_evenly_spaced(&colors, skia_safe::TileMode::Clamp, None);
                    let gradient = Gradient::new(gradient_colors, Interpolation::default());
                    if let Some(shader) = skia_safe::shaders::linear_gradient(
                        (brush.start, brush.end),
                        &gradient,
                        None,
                    ) {
                        glyph_paint.set_shader(shader);
                    }
                }
            } else {
                glyph_paint.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
            }
            if let Some(width) = stroke {
                glyph_paint.set_style(skia_safe::PaintStyle::Stroke);
                glyph_paint.set_stroke_width(width);
            }
            text_style.set_foreground_paint(&glyph_paint);
        }
    }
    let fc = crate::text::font::get_font_collection();
    // Start/middle ellipsis: truncate the STRING, because `set_ellipsis` only marks the end.
    let truncated;
    let mut display_span = None;
    let content = if matches!(
        overflow,
        crate::text::TextOverflow::StartEllipsis | crate::text::TextOverflow::MiddleEllipsis
    ) {
        match truncate_for_ellipsis(content, max_width, &text_style, &fc, overflow) {
            Some((text, span)) => { truncated = (text, span); display_span = Some(truncated.1); truncated.0.as_str() }
            None => content,
        }
    } else {
        content
    };
    let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(content);
    let mut para = builder.build();
    para.layout(layout_width);
    para.set_display_span(display_span);
    para
}

/// The paint-only text extras (shadow, OpenType features) the modifier carries, if any.
pub(crate) struct TextPaintSpec {
    pub shadow: Option<crate::text::TextShadow>,
    pub font_feature_settings: Option<String>,
    pub draw_style: Option<crate::text::TextDrawStyle>,
    pub brush: Option<crate::text::TextBrush>,
    pub transform: Option<crate::text::TextGeometricTransform>,
    pub line_height_style: Option<crate::text::LineHeightStyle>,
}

/// Compose's `LineHeightStyle`, resolved into the two numbers the layout needs.
///
/// `paint_dy` is how far the paragraph is drawn below its box origin (negative = up), and
/// `trim_top` / `trim_bottom` are the slices of extra leading removed from the box height at the
/// first line's top and the last line's bottom. Compose models the extra leading as
/// `lineHeight - fontLineHeight`, split by the alignment's `topRatio`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct LineGeometry {
    pub paint_dy: f32,
    pub trim_top: f32,
    pub trim_bottom: f32,
}

/// The same arithmetic [`line_geometry`] does, for callers that have the numbers but no node.
///
/// Kept in one place so the cached paint path and the uncached fallback cannot drift apart.
pub(crate) fn line_geometry_from(
    line_height_px: Option<f32>,
    style: Option<crate::text::LineHeightStyle>,
    natural_line_height: f32,
) -> LineGeometry {
    let Some(style) = style else { return LineGeometry::default() };
    if style.is_noop() {
        return LineGeometry::default();
    }
    let Some(line_height_px) = line_height_px else {
        return LineGeometry::default();
    };
    let extra = (line_height_px - natural_line_height).max(0.0);
    if extra <= 0.5 {
        return LineGeometry::default();
    }
    let ratio = style.alignment.top_ratio();
    let trim_top = match style.trim {
        crate::text::LineHeightTrim::FirstLineTop | crate::text::LineHeightTrim::Both => ratio * extra,
        _ => 0.0,
    };
    let trim_bottom = match style.trim {
        crate::text::LineHeightTrim::LastLineBottom | crate::text::LineHeightTrim::Both => {
            (1.0 - ratio) * extra
        }
        _ => 0.0,
    };
    LineGeometry {
        paint_dy: (ratio - 0.5) * extra - trim_top,
        trim_top,
        trim_bottom,
    }
}

pub(crate) fn line_geometry(node: &LayoutNode, para: &crate::text::Paragraph) -> LineGeometry {
    // Read the element directly: `text_paint` CLONES the brush and the feature string, and this runs
    // per measure, per paint and per pointer event.
    let Some(style) = line_height_style_of(node) else {
        return LineGeometry::default();
    };
    if style.is_noop() {
        return LineGeometry::default();
    }
    line_geometry_from(text_line_height_px(node), Some(style), para.natural_line_height())
}

/// The `LineHeightStyle` this node carries, copied out without touching the rest of the element.
pub(crate) fn line_height_style_of(node: &LayoutNode) -> Option<crate::text::LineHeightStyle> {
    node.modifier.elements().iter().find_map(|el| match el {
        ModifierElement::TextPaint { line_height_style, .. } => *line_height_style,
        _ => None,
    })
}

/// The `line_height` the payload carries, in logical pixels (the same number the paragraph used).
pub(crate) fn text_line_height_px(node: &LayoutNode) -> Option<f32> {
    node.modifier.elements().iter().find_map(|el| match el {
        ModifierElement::TextContent { line_height, .. } => *line_height,
        _ => None,
    })
}

/// The horizontal glyph transform this node carries, if any (Compose's `textGeometricTransform`).
pub(crate) fn text_transform(node: &LayoutNode) -> Option<crate::text::TextGeometricTransform> {
    // Read the element directly: `text_paint` CLONES the brush and the feature string, and this is
    // called per measure, per paint and per pointer event.
    node.modifier.elements().iter().find_map(|el| match el {
        ModifierElement::TextPaint { transform, .. } => *transform,
        _ => None,
    }).filter(|t| !t.is_identity())
}

pub(crate) fn text_paint(node: &LayoutNode) -> Option<TextPaintSpec> {
    node.modifier.elements().iter().find_map(|el| match el {
        ModifierElement::TextPaint { shadow, font_feature_settings, draw_style, brush, transform, line_height_style } => {
            Some(TextPaintSpec {
                shadow: *shadow,
                font_feature_settings: font_feature_settings.clone(),
                draw_style: *draw_style,
                brush: brush.clone(),
                transform: *transform,
                line_height_style: *line_height_style,
            })
        }
        _ => None,
    })
}

/// Test hook: run [`truncate_for_ellipsis`] with a plain style, without a node.
#[cfg(test)]
pub(crate) fn truncate_for_ellipsis_probe(
    content: &str,
    max_width: f32,
    overflow: crate::text::TextOverflow,
) -> Option<(String, crate::text::DisplaySpan)> {
    let mut style = skia_safe::textlayout::TextStyle::new();
    style.set_font_size(15.0);
    let fc = crate::text::font::get_font_collection();
    truncate_for_ellipsis(content, max_width, &style, &fc, overflow)
}

/// Skia can only ellipsize the END of a line, so start/middle ellipsis truncate the string here.
///
/// Compose's `TextOverflow.StartEllipsis` / `MiddleEllipsis` keep the tail (or both ends) and put
/// the marker at the front (or in the middle). `ParagraphStyle::set_ellipsis` has no such control, so
/// the longest string that fits with a `…` in that position is found by binary search and laid out
/// instead. Returns `None` when the whole text already fits — the caller then lays out the original.
fn truncate_for_ellipsis(
    content: &str,
    max_width: f32,
    text_style: &skia_safe::textlayout::TextStyle,
    fc: &skia_safe::textlayout::FontCollection,
    overflow: crate::text::TextOverflow,
) -> Option<(String, crate::text::DisplaySpan)> {
    let probe = |candidate: &str| -> f32 {
        let mut style = skia_safe::textlayout::ParagraphStyle::new();
        style.set_text_align(skia_safe::textlayout::TextAlign::Left);
        let mut builder = crate::text::ParagraphBuilder::new(&style, fc);
        builder.push_style(text_style);
        builder.add_text(candidate);
        let mut para = builder.build();
        para.layout(f32::MAX);
        para.max_intrinsic_width()
    };
    if probe(content) <= max_width {
        return None;
    }
    // Cut on GRAPHEME CLUSTERS, not chars: a family emoji or a combining mark must not be split, or
    // the truncated string carries a dangling zero-width joiner (measured: "👨\u{200D}…il").
    let mut bounds: Vec<usize> = {
        use unicode_segmentation::UnicodeSegmentation;
        content.grapheme_indices(true).map(|(i, _)| i).collect()
    };
    bounds.push(content.len());
    let clusters = bounds.len() - 1;
    let build = |kept: usize| -> (String, crate::text::DisplaySpan) {
        match overflow {
            crate::text::TextOverflow::StartEllipsis => {
                let tail_start = bounds[clusters - kept];
                let span = crate::text::DisplaySpan {
                    head_bytes: 0,
                    tail_start_byte: tail_start,
                    marker_bytes: "\u{2026}".len(),
                };
                (format!("\u{2026}{}", &content[tail_start..]), span)
            }
            _ => {
                // Middle: keep the head and the tail in equal share, head first.
                let head = kept / 2;
                let tail = kept - head;
                let tail_start = bounds[clusters - tail];
                let span = crate::text::DisplaySpan {
                    head_bytes: bounds[head],
                    tail_start_byte: tail_start,
                    marker_bytes: "\u{2026}".len(),
                };
                (
                    format!(
                        "{}\u{2026}{}",
                        &content[..bounds[head]],
                        &content[tail_start..]
                    ),
                    span,
                )
            }
        }
    };
    // `lo` keeps `0` clusters (the bare marker), `hi` is the first count that does not fit.
    let (mut lo, mut hi) = (0usize, clusters);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if probe(&build(mid).0) <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Some(build(lo))
}

/// Text leaf: build the paragraph laid out at `max_width`, WITHOUT touching the render cache.
///
/// `measure_and_cache_text` and the intrinsic probes both go through this. A probe must not leave
/// a paragraph laid out at its own width in `cached_paragraph` — the renderer reads that cache,
/// and a probe runs with the intrinsic query's width, not the frame's.
pub(crate) fn build_text_paragraph(node: &LayoutNode, max_width: f32) -> Option<crate::text::Paragraph> {
    build_text_paragraph_at(node, max_width, None)
}

/// The same build, with the font size overridden — the `autoSize` fit search needs to lay the leaf
/// out at a candidate size without touching the payload.
pub(crate) fn build_text_paragraph_at(
    node: &LayoutNode,
    max_width: f32,
    font_size_override: Option<f32>,
) -> Option<crate::text::Paragraph> {
    for el in node.modifier.elements() {
        if let ModifierElement::TextContent {
            content, font_size, color, font_weight, font_style, max_lines, align, overflow, soft_wrap,
            letter_spacing, line_height, underline, line_through, font_family, ..
        } = el {
            return Some(build_plain_paragraph(
                content.as_str(),
                font_size_override.unwrap_or(*font_size),
                color,
                *font_weight,
                *font_style,
                *max_lines,
                *align,
                *overflow,
                *soft_wrap,
                *letter_spacing,
                *line_height,
                *underline,
                *line_through,
                font_family.as_deref(),
                text_paint(node),
                max_width,
            ));
        }
    }
    None
}

/// Height reserved below a text leaf that carries TextField supporting text (the supporting line
/// is drawn 4dp below the container and is 16dp tall). Shared by the measure pass and the
/// intrinsic probes so both reserve the same box.
pub(crate) fn supporting_text_height(node: &LayoutNode) -> f32 {
    if node
        .modifier
        .elements()
        .iter()
        .any(|el| matches!(el, ModifierElement::TextFieldVisual { supporting: Some(_), .. }))
    {
        4.0 + 16.0
    } else {
        0.0
    }
}

/// The combined text measure and Paragraph cache.
///
/// Pulls every parameter out of the TextContent modifier (font_size, max_lines, overflow, align),
/// applies them to a ParagraphStyle, builds the Paragraph ONCE, measures it and caches it for the
/// render pass. The old code built it twice (measure + cache_text_paragraph).
/// What one text measure produced, plus the callback the engine fires once the box is final.
///
/// The callback cannot run here: this function sees the paragraph's own size, while Compose's
/// `TextLayoutResult.size` is `constraints.constrain(...)` of it (a `fill_max_width` text reports
/// the filled width, not its intrinsic one). The engine owns the constraints, so it fires the
/// callback after clamping — see the text arm of `measure_node_inner`.
pub(crate) struct TextMeasure {
    /// The paragraph's size, grown to `min_lines`.
    pub size: Size,
    /// The paragraph's own size, before `min_lines` grew the box.
    pub text_size: Size,
    pub line_count: usize,
    pub did_exceed_max_lines: bool,
    /// Width of the widest laid-out line, used for `didOverflowWidth`.
    pub longest_line: f32,
    /// The first and last alphabetic baselines relative to the box top.
    pub first_baseline: f32,
    pub last_baseline: f32,
    pub on_text_layout: Option<std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult) + Send + Sync>>,
    /// The richer sink: the same snapshot plus the paragraph, for Compose's per-line queries.
    pub on_text_layout_layout: Option<
        std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult, &crate::text::TextLayout<'_>) + Send + Sync>,
    >,
}

pub(crate) fn measure_and_cache_text(node: &LayoutNode, max_width: f32) -> TextMeasure {
    measure_and_cache_text_at(node, max_width, None)
}

/// The same measure at a candidate font size (the `autoSize` search); the LAST call caches, so the
/// search has to end on the size it chose.
pub(crate) fn measure_and_cache_text_at(
    node: &LayoutNode,
    max_width: f32,
    font_size_override: Option<f32>,
) -> TextMeasure {
    // A horizontal transform widens what the run DRAWS, so the paragraph is laid out in its own
    // (unscaled) space and the measured box is the scaled one. Compose does the same: the transform
    // is applied to the glyph run, not to the paragraph's layout width.
    let scale_x = text_transform(node).map(|t| t.scale_x.max(0.0001)).unwrap_or(1.0);
    let sink = text_layout_layout_sink(node);
    let layout_width = max_width / scale_x;
    let Some(para) = build_text_paragraph_at(node, layout_width, font_size_override) else {
        return TextMeasure {
            size: Size::ZERO,
            text_size: Size::ZERO,
            line_count: 0,
            did_exceed_max_lines: false,
            longest_line: 0.0,
            first_baseline: 0.0,
            last_baseline: 0.0,
            on_text_layout: None,
            on_text_layout_layout: None,
        };
    };

    let text_size = Size::new(
        (para.max_intrinsic_width() * scale_x).ceil().min(max_width),
        para.height().ceil(),
    );
    let (min_lines, on_text_layout) = text_layout_extras(node);
    let line_count = para.inner_paragraph().line_number();
    let did_exceed_max_lines = para.did_exceed_max_lines();

    // Compose's `minLines`: the box reserves at least that many lines even when the text is shorter.
    // skia-safe does not expose `set_min_lines`, so the RESULT is padded rather than the paragraph —
    // the glyphs still sit at the top, which is what `minLines` does anyway.
    let mut size = text_size;
    if min_lines > line_count.max(1) {
        let line_height = if line_count > 0 {
            text_size.height / line_count as f32
        } else {
            text_size.height
        };
        size.height = size.height.max(line_height * min_lines as f32);
    }

    // Compose's `LineHeightStyle.Trim`: the leading at the first line's top / last line's bottom is
    // removed from the box, which is what makes a trimmed line sit flush with its neighbours.
    let geometry = line_geometry(node, &para);
    if geometry.trim_top > 0.0 || geometry.trim_bottom > 0.0 {
        size.height = (size.height - geometry.trim_top - geometry.trim_bottom).max(0.0);
    }

    let longest_line = para.longest_line() * scale_x;
    let first_baseline = para.alphabetic_baseline();
    let last_baseline = if line_count <= 1 {
        first_baseline
    } else {
        para.get_line_metrics().last().map(|m| m.baseline as f32).unwrap_or(first_baseline)
    };

    *node.cached_paragraph.borrow_mut() = Some(para);
    TextMeasure {
        size,
        text_size,
        line_count,
        did_exceed_max_lines,
        longest_line,
        first_baseline,
        last_baseline,
        on_text_layout,
        on_text_layout_layout: sink,
    }
}

/// The text element's measure-side extras: how many lines to reserve, and the `onTextLayout` sink.
#[allow(clippy::type_complexity)]
fn text_layout_extras(
    node: &LayoutNode,
) -> (
    usize,
    Option<std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult) + Send + Sync>>,
) {
    for el in node.modifier.elements() {
        if let ModifierElement::TextContent {
            min_lines,
            on_text_layout,
            ..
        } = el
        {
            return (*min_lines, on_text_layout.clone());
        }
    }
    (1, None)
}

/// The richer `onTextLayout` sink, if the leaf carries one.
fn text_layout_layout_sink(
    node: &LayoutNode,
) -> Option<std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult, &crate::text::TextLayout<'_>) + Send + Sync>> {
    for el in node.modifier.elements() {
        if let ModifierElement::TextLayoutCallback { callback } = el {
            return Some(callback.clone());
        }
    }
    None
}

/// Remap a character range from the ORIGINAL rich content into the TRUNCATED one.
///
/// `head` / `tail_start` are CHARACTER indices of the kept head prefix and the kept tail suffix in
/// the original; the truncated string is `head + "…" + tail`, so a tail range moves by
/// `head + 1 - tail_start`. A range entirely inside the dropped gap disappears; one that straddles
/// the gap collapses onto the marker.
fn remap_rich_range(
    range: &std::ops::Range<usize>,
    head: usize,
    tail_start: usize,
    total: usize,
) -> Option<std::ops::Range<usize>> {
    let in_head = |i: usize| i < head;
    let in_tail = |i: usize| i >= tail_start && i <= total;
    if in_head(range.start) {
        // Starts in the head: keep it there, and let the end follow whichever side it is on.
        let end = if range.end <= head { range.end } else { head + 1 };
        return if end > range.start { Some(range.start..end) } else { None };
    }
    if in_tail(range.start) {
        let shift = head + 1;
        let s = shift + (range.start - tail_start);
        let e = shift + (range.end.min(total) - tail_start);
        return if e > s { Some(s..e) } else { None };
    }
    // Starts inside the dropped gap: the whole range is invisible.
    None
}

/// The rich-text measure and cache, inline drawables included.
///
/// Pulls the content text, the inline elements and the per-span styles out of the RichTextContent
/// modifier, builds a paragraph whose inline elements sit behind U+FFFC placeholders, applies each
/// span's style and caches the Paragraph and the drawables for the render pass.
pub(crate) fn measure_and_cache_richtext(node: &LayoutNode, max_width: f32) -> Size {
    use skia_safe::textlayout::{ParagraphStyle, PlaceholderAlignment, TextBaseline};
    
    
    let fc = crate::text::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::RichTextContent {
            content, drawables, drawable_ranges, spans,
            align, max_lines, overflow, soft_wrap, line_height, min_lines,
            line_height_style,
        } = el {
            // Compose hands `BasicText` the same paragraph parameters whether the string is
            // annotated or not; this path used to build a bare `ParagraphStyle` and ignore every
            // one of them, so a rich block could only wrap. The rules below are the text leaf's
            // (`build_plain_paragraph`), not a second interpretation: a nowrap ellipsis is a
            // one-line ellipsis, and a nowrap paragraph is laid out at an infinite width so
            // nothing wraps.
            let width_matters = *soft_wrap || overflow.is_ellipsis();
            let effective_max_lines =
                if !*soft_wrap && overflow.is_ellipsis() { 1 } else { *max_lines };
            let mut para_style = ParagraphStyle::new();
            if effective_max_lines < usize::MAX {
                para_style.set_max_lines(effective_max_lines);
            }
            // Rich text keeps the END ellipsis for all three modes: truncating inside a span run
            // would have to move the spans with it, which the plain-leaf truncation (see
            // `truncate_for_ellipsis`) does not do. Documented in D-4.
            if overflow.is_ellipsis() {
                para_style.set_ellipsis("\u{2026}");
            }
            // Only `Justify` reaches the paragraph: `Center`/`Right` are applied by shifting the
            // paint origin, exactly like the plain leaf (see D-3 in docs/text-review.md — doing
            // both would double-apply the shift).
            if width_matters {
                match align {
                    crate::text::TextAlign::Center => { para_style.set_text_align(skia_safe::textlayout::TextAlign::Center); }
                    crate::text::TextAlign::Right => { para_style.set_text_align(skia_safe::textlayout::TextAlign::Right); }
                    crate::text::TextAlign::Justify => { para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify); }
                    crate::text::TextAlign::Left
                    | crate::text::TextAlign::Start
                    | crate::text::TextAlign::End => {}
                }
            }
            let layout_width = if width_matters { max_width.min(f32::MAX) } else { f32::MAX };
            // Start/middle ellipsis on RICH text: truncate the CONTENT first, then remap the spans
            // and the drawable ranges onto the shortened string, so every offset the paragraph sees
            // (spans, placeholder positions) still describes the text it actually lays out. The plain
            // leaf's `truncate_for_ellipsis` cannot be reused verbatim: it works in bytes and knows
            // nothing about spans.
            let original_chars = content.chars().count();
            let mut rich_text: String = content.clone();
            let mut rich_display: Option<crate::text::DisplaySpan> = None;
            let mut rich_spans: Vec<crate::text::RichSpanStyle> = spans.clone();
            let mut rich_ranges: Vec<std::ops::Range<usize>> = drawable_ranges.clone();
            if matches!(
                overflow,
                crate::text::TextOverflow::StartEllipsis | crate::text::TextOverflow::MiddleEllipsis
            ) {
                let build_chars = |kept: usize| -> (String, usize, usize) {
                    let chars: Vec<char> = content.chars().collect();
                    let n = chars.len();
                    match overflow {
                        crate::text::TextOverflow::StartEllipsis => {
                            let tail_start = n - kept;
                            (format!("\u{2026}{}", chars[tail_start..].iter().collect::<String>()), 0, tail_start)
                        }
                        _ => {
                            let head = kept / 2;
                            let tail = kept - head;
                            let tail_start = n - tail;
                            (
                                format!(
                                    "{}\u{2026}{}",
                                    chars[..head].iter().collect::<String>(),
                                    chars[tail_start..].iter().collect::<String>()
                                ),
                                head,
                                tail_start,
                            )
                        }
                    }
                };
                let probe_width = |candidate: &str| -> f32 {
                    let mut style = ParagraphStyle::new();
                    style.set_text_align(skia_safe::textlayout::TextAlign::Left);
                    let mut pb = crate::text::ParagraphBuilder::new(&style, &fc);
                    pb.add_text(candidate);
                    let mut p = pb.build();
                    p.layout(f32::MAX);
                    p.max_intrinsic_width()
                };
                // A run that already fits is left ALONE: keeping every character and adding a marker
                // still fits, and the binary search would otherwise return that (measured: a 20-char
                // run in a 400 px box came back as "…0123456789abcdefghij", 23 bytes).
                let fits = probe_width(content) <= max_width;
                let (text, head, tail_start) = if fits {
                    (content.clone(), 0usize, original_chars)
                } else {
                    let (mut lo, mut hi) = (0usize, original_chars);
                    while lo < hi {
                        let mid = (lo + hi + 1) / 2;
                        if probe_width(&build_chars(mid).0) <= max_width { lo = mid; } else { hi = mid - 1; }
                    }
                    build_chars(lo)
                };
                if !fits {
                    rich_text = text;
                    rich_display = Some(crate::text::DisplaySpan {
                        head_bytes: content.chars().take(head).map(|c| c.len_utf8()).sum(),
                        tail_start_byte: content.chars().take(tail_start).map(|c| c.len_utf8()).sum(),
                        marker_bytes: "\u{2026}".len(),
                    });
                }
                rich_spans = spans
                    .iter()
                    .filter_map(|s| {
                        remap_rich_range(&(s.start..s.end), head, tail_start, original_chars).map(|r| {
                            let mut c = s.clone();
                            c.start = r.start;
                            c.end = r.end;
                            c
                        })
                    })
                    .collect();
                rich_ranges = drawable_ranges
                    .iter()
                    .filter_map(|r| remap_rich_range(r, head, tail_start, original_chars))
                    .collect();
            }
            let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);

            // Walk the spans, tracking the drawable ranges
            let content = &rich_text;
            let spans = &rich_spans;
            let drawable_ranges = &rich_ranges;
            let chars: Vec<char> = content.chars().collect();
            let mut di = 0usize;
            let mut span_idx = 0usize;
            let total = chars.len();
            let mut ci = 0usize;
            while ci < total {
                // Whether this character belongs to a drawable
                if di < drawable_ranges.len() && ci >= drawable_ranges[di].start && ci < drawable_ranges[di].end {
                    // Placeholder: add it once at the start of the range and skip the rest of it.
                    //
                    // The string handed to `add_placeholder` is NOT drawn (Skia sees one U+FFFC), but
                    // its LENGTH advances the builder's real-index space, and that space has to agree
                    // with `content` byte for byte or every later index drifts. The range's own text is
                    // therefore what gets passed: a one-char image still occupies 3 bytes (U+FFFC), and
                    // a multi-char placeholder (`RichTextScope::placeholder("XYZ", ..)`) occupies all
                    // of its own bytes. Measured before this fix: `"ab" + image + "cd"` put `"cd"`
                    // at real bytes 3..5 where the content has it at 5..7, so a selection over the tail
                    // returned no rectangles at all.
                    if ci == drawable_ranges[di].start {
                        if let Some(s) = spans.iter().find(|s| s.start <= ci && s.end > ci) {
                            builder.push_style(&styled_for_span(s, *line_height));
                        }
                        let d = drawables[di].clone();
                        let placeholder_text: String =
                            chars[drawable_ranges[di].start..drawable_ranges[di].end.min(total)].iter().collect();
                        builder.add_placeholder(&placeholder_text, d, &Some((PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0)));
                        if let Some(_s) = spans.iter().find(|s| s.start <= ci && s.end > ci) { builder.pop(); }
                    }
                    ci += 1;
                    if ci >= drawable_ranges[di].end { di += 1; }
                    continue;
                }

                // Text: look the span up first
                while span_idx < spans.len() && spans[span_idx].end <= ci { span_idx += 1; }
                if span_idx < spans.len() && spans[span_idx].start <= ci && ci < spans[span_idx].end {
                    let run_end = spans[span_idx].end.min(total);
                    let text: String = chars[ci..run_end].iter().collect();
                    if !text.is_empty() {
                        builder.push_style(&styled_for_span(&spans[span_idx], *line_height));
                        builder.add_text(&text);
                        builder.pop();
                    }
                    ci = run_end;
                    continue;
                }

                // Text with no span
                builder.add_text(&chars[ci].to_string());
                ci += 1;
            }

            let mut para = builder.build();
            para.layout(layout_width);
            para.set_display_span(rich_display);
            let text_size = Size::new(
                para.max_intrinsic_width().ceil().min(max_width),
                para.height().ceil(),
            );
            // `minLines` reserves the box height, the way the plain leaf does it (skia-safe exposes
            // no `set_min_lines`): pad the RESULT, leave the glyphs at the top.
            let line_count = para.inner_paragraph().line_number();
            let mut size = text_size;
            // Compose's `lineHeightStyle.Trim`: the extra leading at the ends comes off the box.
            let geometry = line_geometry_from(*line_height, *line_height_style, para.natural_line_height());
            if geometry.trim_top > 0.0 || geometry.trim_bottom > 0.0 {
                size.height = (size.height - geometry.trim_top - geometry.trim_bottom).max(0.0);
            }
            if *min_lines > line_count.max(1) {
                let line_height_avg = if line_count > 0 {
                    text_size.height / line_count as f32
                } else {
                    text_size.height
                };
                size.height = size.height.max(line_height_avg * *min_lines as f32);
            }
            *node.cached_paragraph.borrow_mut() = Some(para);
            return size;
        }
    }
    Size::ZERO
}

/// A span's Skia style, with the paragraph's fixed line height applied when the span has none.
///
/// Compose's `lineHeight` is a paragraph-level parameter that a span-level override wins over
/// (`TextStyle.lineHeight` per span is what `RichSpanStyle::height_multiple` models), so a span
/// that set its own height keeps it.
fn styled_for_span(s: &RichSpanStyle, line_height: Option<f32>) -> SkTextStyle {
    let mut ts = to_sktextstyle(s);
    if let Some(lh) = line_height {
        if lh > 0.0 && s.height_multiple == 0.0 && s.font_size > 0.0 {
            ts.set_height(lh / s.font_size);
            ts.set_height_override(true);
            // Centred leading, the same rule the plain leaf uses (D-2).
            ts.set_half_leading(true);
        }
    }
    ts
}

/// Converts a RichSpanStyle into a Skia TextStyle.
pub(crate) fn to_sktextstyle(s: &RichSpanStyle) -> SkTextStyle {
    let mut ts = SkTextStyle::new();
    ts.set_font_size(s.font_size);
    ts.set_color(skia_safe::Color::from_argb(s.color.a, s.color.r, s.color.g, s.color.b));

    // Weight, slant, width
    let slant = match s.font_style {
        FontSlant::Upright => skia_safe::font_style::Slant::Upright,
        FontSlant::Italic => skia_safe::font_style::Slant::Italic,
        FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
    };
    ts.set_font_style(SkFontStyle::new(s.font_weight.value().into(), s.font_width.into(), slant));

    // Decoration
    let mut deco: skia_safe::textlayout::TextDecoration = skia_safe::textlayout::TextDecoration::default();
    if s.underline { deco |= skia_safe::textlayout::TextDecoration::UNDERLINE; }
    if s.overline { deco |= skia_safe::textlayout::TextDecoration::OVERLINE; }
    if s.strikethrough { deco |= skia_safe::textlayout::TextDecoration::LINE_THROUGH; }
    ts.set_decoration_type(deco);
    if let Some(c) = &s.decoration_color {
        ts.set_decoration_color(skia_safe::Color::from_argb(c.a, c.r, c.g, c.b));
    }
    if let Some(st) = s.decoration_style {
        use crate::text::DecoStyle;
        let sk = match st {
            DecoStyle::Solid => skia_safe::textlayout::TextDecorationStyle::Solid,
            DecoStyle::Double => skia_safe::textlayout::TextDecorationStyle::Double,
            DecoStyle::Dotted => skia_safe::textlayout::TextDecorationStyle::Dotted,
            DecoStyle::Dashed => skia_safe::textlayout::TextDecorationStyle::Dashed,
            DecoStyle::Wavy => skia_safe::textlayout::TextDecorationStyle::Wavy,
        };
        ts.set_decoration_style(sk);
    }
    if let Some(m) = s.decoration_mode {
        use crate::text::DecoMode;
        let sk = match m {
            DecoMode::Gaps => skia_safe::textlayout::TextDecorationMode::Gaps,
            DecoMode::Through => skia_safe::textlayout::TextDecorationMode::Through,
        };
        ts.set_decoration_mode(sk);
    }

    // Baseline shift (shift = font_size * multiplier)
    if s.baseline_shift != 0.0 {
        ts.set_baseline_shift(s.baseline_shift * s.font_size);
    }

    // Spacing
    if s.letter_spacing != 0.0 { ts.set_letter_spacing(s.letter_spacing); }
    if s.word_spacing != 0.0 { ts.set_word_spacing(s.word_spacing); }
    if s.height_multiple != 0.0 { ts.set_height(s.height_multiple); ts.set_height_override(true); }
    if s.half_leading { ts.set_half_leading(true); }

    // Font family
    if !s.font_families.is_empty() {
        ts.set_font_families(&s.font_families.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }

    // Render hinting
    if let Some(e) = s.font_edging {
        use crate::text::FontEdge;
        let sk = match e {
            FontEdge::Alias => skia_safe::font::Edging::Alias,
            FontEdge::AntiAlias => skia_safe::font::Edging::AntiAlias,
            FontEdge::SubpixelAntiAlias => skia_safe::font::Edging::SubpixelAntiAlias,
        };
        ts.set_font_edging(sk);
    }
    if let Some(h) = s.font_hinting {
        use crate::text::FontHint;
        let sk = match h {
            FontHint::None => skia_safe::FontHinting::None,
            FontHint::Slight => skia_safe::FontHinting::Slight,
            FontHint::Normal => skia_safe::FontHinting::Normal,
            FontHint::Full => skia_safe::FontHinting::Full,
        };
        ts.set_font_hinting(sk);
    }
    if s.subpixel { ts.set_subpixel(true); }

    // Foreground and background
    if let Some(fg) = &s.foreground_color {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(fg.a, fg.r, fg.g, fg.b));
        paint.set_style(skia_safe::paint::Style::Fill);
        ts.set_foreground_paint(&paint);
    }
    if let Some(bg) = &s.background {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
        paint.set_style(skia_safe::paint::Style::Fill);
        ts.set_background_paint(&paint);
    }

    // locale
    if let Some(loc) = &s.locale {
        ts.set_locale(loc);
    }

    ts
}

// ── Main-axis spacing (shared by Column and Row) ──

/// Computes an Arrangement's spacing: returns (extra gap between items, the leading offset at
/// either end). Column passes the vertical main axis, Row the horizontal one — measure_flex decides.
pub fn compute_spacing(
    arrangement: crate::layout::Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        // Compose's `Arrangement.Absolute.*` distributes exactly like the mirroring variant; the
        // difference is only that the container skips the RTL mirror (see `measure_flex`).
        crate::layout::Arrangement::Absolute(absolute) => {
            return relative_spacing(absolute.relative(), remaining, gap_count);
        }
        // Compose's `aligned(alignment)`: the gap between children is the container's own
        // `spacing`, and the leftover main-axis space all goes to the alignment. `Stretch` cannot
        // be expressed on the main axis and behaves as `Center`.
        crate::layout::Arrangement::Aligned(alignment) => {
            return (0.0, aligned_leading(alignment, remaining));
        }
        _ => {}
    }
    relative_spacing(arrangement, remaining, gap_count)
}

/// Where `Arrangement.aligned(alignment)` puts the leftover main-axis space.
fn aligned_leading(alignment: crate::layout::Alignment, remaining: f32) -> f32 {
    match alignment {
        crate::layout::Alignment::Start => 0.0,
        crate::layout::Alignment::End => remaining,
        // `Stretch` cannot be expressed on the main axis; Compose's `Alignment.Horizontal` has no
        // such value either, so it behaves as Center instead of panicking.
        crate::layout::Alignment::Center | crate::layout::Alignment::Stretch => remaining / 2.0,
    }
}

/// The six mirroring distributions (Compose's `Arrangement.Start` … `SpaceEvenly`).
fn relative_spacing(
    arrangement: crate::layout::Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        // Unreachable from `compute_spacing` (it unwraps both payloads first), but the match has to
        // stay exhaustive so a new variant cannot be forgotten here.
        crate::layout::Arrangement::Absolute(absolute) => {
            relative_spacing(absolute.relative(), remaining, gap_count)
        }
        crate::layout::Arrangement::Aligned(alignment) => (0.0, aligned_leading(alignment, remaining)),
        crate::layout::Arrangement::Start => (0.0, 0.0),
        crate::layout::Arrangement::End => (0.0, remaining),
        crate::layout::Arrangement::Center => (0.0, remaining / 2.0),
        crate::layout::Arrangement::SpaceBetween => {
            if gap_count > 0 {
                (remaining / gap_count as f32, 0.0)
            } else {
                // Compose's placeSpaceBetween divides by maxOf(size.lastIndex, 1), so a single
                // child has one gap and sits at the START (LTR) — it is not centred. The RTL mirror
                // the container applies afterwards turns that into the end edge, which is what
                // Compose's reverseInput branch does too (Arrangement.kt:680-703).
                (0.0, 0.0)
            }
        }
        crate::layout::Arrangement::SpaceAround => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space / 2.0) // (gap between items, half of it at either edge)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        crate::layout::Arrangement::SpaceEvenly => {
            if gap_count > 0 {
                // n children leave n+1 equal gaps: one before the first child and one after the
                // last, on top of the n-1 between pairs. Dividing by `gap_count + 1` left the
                // trailing gap out — measured on the docked picker's month list, whose four 36 dp
                // rows came out at leading 47.75 / pitch 47.75 / trailing 0 in a 335 dp panel.
                let space = remaining / (gap_count + 2) as f32;
                (space, space) // the same gap between items and at the edges
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}

#[cfg(test)]
mod truncate_tests {
    /// The character-range remap behind the rich truncation: head ranges stay, tail ranges shift by
    /// the marker, a range inside the dropped gap disappears, and a straddling one collapses onto the
    /// marker.
    #[test]
    fn the_rich_truncation_remap_moves_spans_and_drawables() {
        use crate::text::TextOverflow;
        // content "0123456789" (10 chars), start ellipsis keeping the last 4: "…6789".
        let head = 0usize;
        let tail_start = 6usize;
        let total = 10usize;
        let remap = |s: usize, e: usize| crate::layout::node::measure::paragraph::remap_rich_range(&(s..e), head, tail_start, total);
        // Inside the kept tail: shifts by head + marker = 1.
        assert_eq!(remap(6, 8), Some(1..3));
        assert_eq!(remap(6, 10), Some(1..5));
        // Entirely dropped: gone.
        assert_eq!(remap(1, 4), None);
        // Straddling the marker: collapses onto it.
        assert_eq!(remap(2, 7), None, "starts in the gap, so nothing visible");
        // A middle ellipsis: head 4 kept, tail from 8.
        let middle = |s: usize, e: usize| crate::layout::node::measure::paragraph::remap_rich_range(&(s..e), 4, 8, total);
        assert_eq!(middle(0, 2), Some(0..2), "inside the head");
        assert_eq!(middle(2, 5), Some(2..5), "straddles into the marker, kept as head up to the marker");
        assert_eq!(middle(8, 10), Some(5..7), "inside the tail, shifted by head + marker");
        assert_eq!(middle(5, 7), None, "inside the dropped gap");
        let _ = TextOverflow::MiddleEllipsis;
    }
    use super::*;

    /// Truncation must cut on grapheme clusters: a family emoji is one cluster of seven code
    /// points, and cutting by chars left a dangling zero-width joiner right before the marker
    /// (measured: `"👨\u{200D}…il"`).
    #[test]
    fn truncation_never_splits_a_grapheme_cluster() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let text = format!("{family} tail");
        for overflow in [
            crate::text::TextOverflow::StartEllipsis,
            crate::text::TextOverflow::MiddleEllipsis,
        ] {
            let (out, span) = truncate_for_ellipsis_probe(&text, 40.0, overflow).expect("it must truncate");
            // The span must describe the string that was actually returned.
            assert!(
                out == format!("{}{}{}", &text[..span.head_bytes], "\u{2026}", &text[span.tail_start_byte..]),
                "the span rebuilds the display text: {out:?} {span:?}"
            );
            let marker = out.find('\u{2026}').expect("the marker is in the result");
            let before = out[..marker].chars().next_back();
            let after = out[marker..].chars().nth(1);
            assert!(
                before != Some('\u{200D}') && after != Some('\u{200D}'),
                "no half cluster next to the marker ({overflow:?}): {out:?}"
            );
            // The kept side is whole clusters: re-segmenting it yields no lone ZWJ run either.
            assert!(
                !out.ends_with('\u{200D}') && !out.starts_with('\u{200D}'),
                "the head and the tail are cluster aligned: {out:?}"
            );
        }
    }

    /// A string that already fits is left alone (no marker, no truncation).
    #[test]
    fn a_fitting_string_is_not_truncated() {
        assert!(truncate_for_ellipsis_probe("ok", 400.0, crate::text::TextOverflow::StartEllipsis).is_none());
    }
}

