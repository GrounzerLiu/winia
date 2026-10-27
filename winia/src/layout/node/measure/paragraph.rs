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
    max_width: f32,
) -> crate::text::Paragraph {
    use skia_safe::textlayout::ParagraphStyle;

    let mut para_style = ParagraphStyle::new();

    // Compose's `TextDelegate.layoutText` (TextDelegate.kt:160-181): the constraint width matters when
    // the text WRAPS or when it ELLIPSIZES, and a nowrap ellipsis is a ONE-LINE ellipsis — native text
    // layout cannot produce several of them, so Compose overwrites maxLines with 1 there. Laying a
    // nowrap ellipsis out at an infinite width (what this did) makes Skia believe the line fits and no
    // ellipsis is ever produced.
    let width_matters = soft_wrap || overflow == crate::text::TextOverflow::Ellipsis;
    let effective_max_lines = if !soft_wrap && overflow == crate::text::TextOverflow::Ellipsis {
        1
    } else {
        max_lines
    };
    if effective_max_lines < usize::MAX {
        para_style.set_max_lines(effective_max_lines);
    }

    // ellipsis overflow: show an ellipsis when the text does not fit
    if overflow == crate::text::TextOverflow::Ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    // justify alignment
    if align == crate::text::TextAlign::Justify {
        para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify);
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
    let fc = crate::text::font::get_font_collection();
    let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(content);
    let mut para = builder.build();
    para.layout(layout_width);
    para
}

/// Text leaf: build the paragraph laid out at `max_width`, WITHOUT touching the render cache.
///
/// `measure_and_cache_text` and the intrinsic probes both go through this. A probe must not leave
/// a paragraph laid out at its own width in `cached_paragraph` — the renderer reads that cache,
/// and a probe runs with the intrinsic query's width, not the frame's.
pub(crate) fn build_text_paragraph(node: &LayoutNode, max_width: f32) -> Option<crate::text::Paragraph> {
    for el in node.modifier.elements() {
        if let ModifierElement::TextContent {
            content, font_size, color, font_weight, font_style, max_lines, align, overflow, soft_wrap,
            letter_spacing, line_height, underline, line_through, font_family, ..
        } = el {
            return Some(build_plain_paragraph(
                content.as_str(),
                *font_size,
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
pub(crate) fn measure_and_cache_text(node: &LayoutNode, max_width: f32) -> Size {
    let Some(para) = build_text_paragraph(node, max_width) else {
        return Size::ZERO;
    };

    let text_size = Size::new(
        para.max_intrinsic_width().ceil().min(max_width),
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

    *node.cached_paragraph.borrow_mut() = Some(para);

    // Compose's `onTextLayout`: report what this pass laid out (never from an intrinsic probe — that
    // is a question about a hypothetical width, not a frame).
    if let Some(callback) = on_text_layout {
        callback(&crate::text::TextLayoutResult {
            size,
            text_size,
            line_count,
            did_exceed_max_lines,
        });
    }
    size
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

/// The rich-text measure and cache, inline drawables included.
///
/// Pulls the content text, the inline elements and the per-span styles out of the RichTextContent
/// modifier, builds a paragraph whose inline elements sit behind U+FFFC placeholders, applies each
/// span's style and caches the Paragraph and the drawables for the render pass.
pub(crate) fn measure_and_cache_richtext(node: &LayoutNode, max_width: f32) -> Size {
    use skia_safe::textlayout::{ParagraphStyle, PlaceholderAlignment, TextBaseline};
    
    
    let fc = crate::text::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::RichTextContent { content, drawables, drawable_ranges, spans } = el {
            let para_style = ParagraphStyle::new();
            let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);

            // Walk the spans, tracking the drawable ranges
            let chars: Vec<char> = content.chars().collect();
            let mut di = 0usize;
            let mut span_idx = 0usize;
            let total = chars.len();
            let mut ci = 0usize;
            while ci < total {
                // Whether this character belongs to a drawable
                if di < drawable_ranges.len() && ci >= drawable_ranges[di].start && ci < drawable_ranges[di].end {
                    // Placeholder: add it once at the start of the range and skip the rest of it
                    if ci == drawable_ranges[di].start {
                        if let Some(s) = spans.iter().find(|s| s.start <= ci && s.end > ci) {
                            builder.push_style(&to_sktextstyle(s));
                        }
                        let d = drawables[di].clone();
                        builder.add_placeholder(" ", d, &Some((PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0)));
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
                        builder.push_style(&to_sktextstyle(&spans[span_idx]));
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
            para.layout(max_width);
            let size = Size::new(
                para.max_intrinsic_width().ceil().min(max_width),
                para.height().ceil(),
            );
            *node.cached_paragraph.borrow_mut() = Some(para);
            return size;
        }
    }
    Size::ZERO
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


