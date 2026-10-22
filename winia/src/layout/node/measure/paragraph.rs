use super::*;

/// 构建普通文本段落（测量与绘制共用——单一事实来源）。
///
/// 从 TextStyle 参数构造 skia Paragraph（含 max_lines/ellipsis/justify/字重/倾斜），
/// 并按 soft_wrap 决定布局宽度。测量期（node.rs）与绘制兜底（render.rs）都调此函数，
/// 避免两处独立构造导致样式不一致。
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
    max_width: f32,
) -> crate::text::Paragraph {
    use skia_safe::textlayout::ParagraphStyle;

    let mut para_style = ParagraphStyle::new();

    // max_lines：限制行数
    if max_lines < usize::MAX {
        para_style.set_max_lines(max_lines);
    }

    // ellipsis overflow：超出时显示省略号
    if overflow == crate::text::TextOverflow::Ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    // justify alignment
    if align == crate::text::TextAlign::Justify {
        para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify);
    }

    // soft_wrap=false: 无限宽度排版，不换行
    let layout_width = if soft_wrap { max_width } else { f32::MAX };

    let mut text_style = skia_safe::textlayout::TextStyle::new();
    text_style.set_font_size(font_size);
    // IMPORTANT: 设置文字颜色（Skia TextStyle 默认白色，不设的话画在白色背景上不可见）
    text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
    // 字间距（对标 Compose TextStyle.letterSpacing——逻辑像素）
    if letter_spacing != 0.0 {
        text_style.set_letter_spacing(letter_spacing);
    }
    // 行高（对标 Compose TextStyle.lineHeight——固定 px；skia 是倍数语义，
    // set_height(multiplier) + set_height_override(true)——override 才强制
    // 行高生效（与 RichText 路径 node.rs:1284 一致，否则被字体默认行高覆盖））
    if let Some(lh) = line_height {
        if lh > 0.0 && font_size > 0.0 {
            text_style.set_height(lh / font_size);
            text_style.set_height_override(true);
        }
    }
    // 设置字重和倾斜
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
            letter_spacing, line_height,
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

/// 合并的文本测量 + Paragraph 缓存。
///
/// 从 TextContent modifier 中提取所有参数（font_size、max_lines、overflow、align），
/// 在 ParagraphStyle 上正确设置后一次创建 Paragraph，测量尺寸并缓存供渲染复用。
/// 消除旧代码中 `measurer.measure()` + `cache_text_paragraph()` 重复创建的开销。
pub(crate) fn measure_and_cache_text(node: &LayoutNode, max_width: f32) -> Size {
    let Some(para) = build_text_paragraph(node, max_width) else {
        return Size::ZERO;
    };

    let size = Size::new(
        para.max_intrinsic_width().ceil().min(max_width),
        para.height().ceil(),
    );

    *node.cached_paragraph.borrow_mut() = Some(para);
    size
}

/// 富文本测量 + 缓存（含内联 drawable）。
///
/// 从 RichTextContent modifier 中提取内容文本、内联元素列表和每段样式，
/// 使用 Skia ParagraphBuilder 构建带 U+FFFC 占位符的段落，
/// 对每个片段应用对应的样式后缓存 Paragraph 和 drawables 供渲染复用。
pub(crate) fn measure_and_cache_richtext(node: &LayoutNode, max_width: f32) -> Size {
    use skia_safe::textlayout::{ParagraphStyle, PlaceholderAlignment, TextBaseline};
    
    
    let fc = crate::text::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::RichTextContent { content, drawables, drawable_ranges, spans } = el {
            let para_style = ParagraphStyle::new();
            let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);

            // 按 span 迭代 + drawable 范围
            let chars: Vec<char> = content.chars().collect();
            let mut di = 0usize;
            let mut span_idx = 0usize;
            let total = chars.len();
            let mut ci = 0usize;
            while ci < total {
                // 是否在 drawable 范围内
                if di < drawable_ranges.len() && ci >= drawable_ranges[di].start && ci < drawable_ranges[di].end {
                    // 占位符（只对范围起点做一次 add_placeholder，跳过剩余字符）
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

                // 文本：先查 span
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

                // 无样式文本
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

/// 将 RichSpanStyle 转为 Skia TextStyle
pub(crate) fn to_sktextstyle(s: &RichSpanStyle) -> SkTextStyle {
    let mut ts = SkTextStyle::new();
    ts.set_font_size(s.font_size);
    ts.set_color(skia_safe::Color::from_argb(s.color.a, s.color.r, s.color.g, s.color.b));

    // 字重/字型/字宽
    let slant = match s.font_style {
        FontSlant::Upright => skia_safe::font_style::Slant::Upright,
        FontSlant::Italic => skia_safe::font_style::Slant::Italic,
        FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
    };
    ts.set_font_style(SkFontStyle::new(s.font_weight.value().into(), s.font_width.into(), slant));

    // 装饰线
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

    // 基线偏移（shift = font_size * multiplier）
    if s.baseline_shift != 0.0 {
        ts.set_baseline_shift(s.baseline_shift * s.font_size);
    }

    // 间距
    if s.letter_spacing != 0.0 { ts.set_letter_spacing(s.letter_spacing); }
    if s.word_spacing != 0.0 { ts.set_word_spacing(s.word_spacing); }
    if s.height_multiple != 0.0 { ts.set_height(s.height_multiple); ts.set_height_override(true); }
    if s.half_leading { ts.set_half_leading(true); }

    // 字体族
    if !s.font_families.is_empty() {
        ts.set_font_families(&s.font_families.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }

    // 渲染精度
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

    // 前景/背景
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

// ── 主轴间距计算（Column/Row 共用）──

/// 计算 Arrangement 的间距分配：返回 (元素间额外间距, 首/尾 leading offset)。
/// Column 垂直主轴 / Row 水平主轴——由调用方（measure_flex）确定主轴分量。
pub fn compute_spacing(
    arrangement: crate::layout::Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        crate::layout::Arrangement::Start => (0.0, 0.0),
        crate::layout::Arrangement::End => (0.0, remaining),
        crate::layout::Arrangement::Center => (0.0, remaining / 2.0),
        crate::layout::Arrangement::SpaceBetween => {
            if gap_count > 0 {
                (remaining / gap_count as f32, 0.0)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        crate::layout::Arrangement::SpaceAround => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space / 2.0) // (元素间间距, 首/尾边缘间距 = 一半)
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
                (space, space) // 元素间与边缘间距相等
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}


