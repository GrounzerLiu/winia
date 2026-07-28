//! 渲染管线 — 遍历 LayoutNode 树绘制到 Skia Canvas
//!
//! 两阶段渲染：
//!   Phase 1: 非 BackdropBlur 内容 + 收集背景模糊区域
//!   Phase 2: snapshot → 每个模糊区域 crop → blur → 画回 + 画子节点

use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use crate::modifier::Dimension;
use skia_safe::{Canvas, Color4f, Paint, RRect, Rect};
use skia_safe::image_filters;
use skia_safe::textlayout::{
    ParagraphBuilder, ParagraphStyle, TextStyle,
};

// ── 入口 ──

pub fn render(root: &LayoutNode, canvas: &Canvas) {
    let mut backdrop_regions = Vec::new();
    // Phase 1: 非背景模糊内容 + 收集模糊区域
    render_pass1(root, canvas, 0.0, 0.0, &mut backdrop_regions, false);
    // Phase 2: 背景模糊
    if !backdrop_regions.is_empty() {
        render_backdrop_blur(canvas, &backdrop_regions);
    }
}

// ── 视觉 Modifier 渲染（Background / Border / TextContent）──

struct TextParams<'a> {
    content: &'a str,
    font_size: f32,
    color: &'a crate::modifier::Color,
    font_weight: crate::ui::text::FontWeight,
    font_style: crate::ui::text::FontSlant,
    max_lines: usize,
    align: crate::ui::TextAlign,
    overflow: crate::ui::TextOverflow,
}

/// 渲染 Background / Border / 提取 TextContent
fn render_modifier_element<'a>(
    canvas: &Canvas,
    el: &'a ModifierElement,
    rect: Rect,
    x: f32, y: f32, w: f32, h: f32,
) -> Option<TextParams<'a>> {
    match el {
        ModifierElement::Background { color, shape } => {
            draw_background(canvas, rect, color, shape);
            None
        }
        ModifierElement::Border { width, color, shape } => {
            draw_border(canvas, x, y, w, h, *width, color, shape);
            None
        }
        ModifierElement::TextContent { content, font_size, color, font_weight, font_style, max_lines, align, overflow } => {
            Some(TextParams {
                content, font_size: *font_size, color,
                font_weight: *font_weight, font_style: *font_style,
                max_lines: *max_lines, align: *align, overflow: *overflow,
            })
        }
        _ => None,
    }
}

// ── Phase 1: 正常渲染（非 BackdropBlur 节点）──

fn render_pass1<'a>(
    node: &'a LayoutNode,
    canvas: &Canvas,
    parent_x: f32, parent_y: f32,
    backdrop_regions: &mut Vec<(f32, f32, f32, f32, f32, &'a LayoutNode)>,
    backdrop_pass: bool,
) {
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;
    if w <= 0.0 || h <= 0.0 { return; }

    let rect = Rect::new(x, y, x + w, y + h);
    let mut blur_radius: Option<f32> = None;
    let mut is_backdrop = false;
    let mut text: Option<(&str, f32, &crate::modifier::Color, usize, crate::ui::TextAlign, crate::ui::TextOverflow, crate::ui::text::FontWeight, crate::ui::text::FontSlant)> = None;
    let mut scroll_offset_v: Option<f32> = None;
    let mut scroll_offset_h: Option<f32> = None;

    for el in node.modifier.elements() {
        match el {
            ModifierElement::Blur { radius } if !backdrop_pass => {
                blur_radius = Some(*radius);
            }
            ModifierElement::BackdropBlur { radius } if !backdrop_pass => {
                is_backdrop = true;
                backdrop_regions.push((x, y, w, h, *radius, node));
            }
            ModifierElement::VerticalScroll { state } if !backdrop_pass => {
                scroll_offset_v = Some(state.get());
            }
            ModifierElement::HorizontalScroll { state } if !backdrop_pass => {
                scroll_offset_h = Some(state.get());
            }
            el => {
                if let Some(tp) = render_modifier_element(canvas, el, rect, x, y, w, h) {
                    text = Some((tp.content, tp.font_size, tp.color, tp.max_lines, tp.align, tp.overflow, tp.font_weight, tp.font_style));
                }
            }
        }
    }

    // 内容模糊：saveLayer
    if !backdrop_pass {
        if let Some(r) = blur_radius {
        let mut paint = Paint::default();
        paint.set_image_filter(image_filters::blur((r, r), skia_safe::TileMode::Clamp, None, None));
        let rec = skia_safe::canvas::SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }
    }

    if let Some((content, font_size, color, max_lines, align, overflow, font_weight, font_style)) = text {
        // 优先用测量阶段缓存的 Paragraph（避免重建）
        if let Some(mut para) = node.cached_paragraph.borrow_mut().take() {
            // 用节点实际宽度重新 layout（测量阶段的排版宽度是约束 max_width，
            // 渲染时确保与节点 measured_size 对齐）
            para.layout(w);
            let x_off = match align {
                crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => x,
                crate::ui::TextAlign::Center => x + (w - para.max_intrinsic_width()).max(0.0) / 2.0,
                crate::ui::TextAlign::Right => x + (w - para.max_intrinsic_width()).max(0.0),
            };
            para.paint(canvas, (x_off, y));
        } else {
            draw_text(canvas, content, font_size, color, font_weight, font_style, x, y, w, max_lines, align, overflow);
        }
    }
    if node.focused {
        draw_focus(canvas, rect);
    }

    // Scroll clip + translate
    let mut scrolled = false;
    if scroll_offset_v.is_some() || scroll_offset_h.is_some() {
        // clip 用 visible 尺寸，优先从 modifier 中提取 Size 或 FillMax 信息
        let mut cw = w;
        let mut ch = h;
        for el in node.modifier.elements() {
            match el {
                ModifierElement::Size { width, height } => {
                    if let Dimension::Fixed(fw) = width { cw = *fw; }
                    if let Dimension::Fixed(fh) = height { ch = *fh; }
                }
                ModifierElement::FillMaxWidth | ModifierElement::FillMaxSize => {
                    cw = w; // measured width = parent max width
                }
                ModifierElement::FillMaxHeight => {
                    ch = h;
                }
                _ => {}
            }
        }
        let clip_rect = Rect::new(x, y, x + cw, y + ch);
        let dx = -scroll_offset_h.unwrap_or(0.0);
        let dy = -scroll_offset_v.unwrap_or(0.0);
        canvas.save();
        canvas.clip_rect(clip_rect, None, Some(false));
        canvas.translate((dx, dy));
        scrolled = true;
    }

    // 穿行子节点（背景模糊节点跳过子节点——Phase 2 处理）
    if !is_backdrop {
        for child in &node.children {
            render_pass1(child, canvas, x, y, backdrop_regions, false);
        }
    }

    if scrolled {
        canvas.restore();
    }

    if blur_radius.is_some() {
        canvas.restore();
    }
}

// ── Phase 2: 背景模糊 ──

fn render_backdrop_blur(
    canvas: &Canvas,
    regions: &[(f32, f32, f32, f32, f32, &LayoutNode)],
) {
    // 取整张 surface snapshot（只回读一次）
    let mut snap_bounds: Option<Rect> = None;
    for &(x, y, w, h, r, _) in regions {
        let m = r * 2.0;
        let b = Rect::new((x - m).max(0.0), (y - m).max(0.0), x + w + m, y + h + m);
        snap_bounds = Some(match snap_bounds {
            Some(prev) => Rect::new(
                prev.left.min(b.left), prev.top.min(b.top),
                prev.right.max(b.right), prev.bottom.max(b.bottom),
            ),
            None => b,
        });
    }

    let snapshot = if let Some(bounds) = snap_bounds {
        let bi = skia_safe::IRect::from_ltrb(
            bounds.left as i32, bounds.top as i32,
            bounds.right as i32, bounds.bottom as i32,
        );
        if bi.width() > 0 && bi.height() > 0 {
            surface_snapshot(canvas, bi)
        } else {
            None
        }
    } else {
        None
    };

    // 为每个背景模糊区域画回
    for &(x, y, w, h, radius, backdrop_node) in regions {
        if let Some(ref snap) = snapshot {
            let mut paint = Paint::default();
            paint.set_image_filter(image_filters::blur((radius, radius), skia_safe::TileMode::Clamp, None, None));
            let m = radius * 2.0;
            let src = Rect::new((x - m).max(0.0), (y - m).max(0.0), x + w + m, y + h + m);
            canvas.draw_image_rect(snap, None, &src, &paint);
        }
        // 画回模糊节点自己的子节点
        for child in &backdrop_node.children {
            render_pass1(child, canvas, x, y, &mut Vec::new(), true);
        }
    }
}

// ── 辅助函数 ──

impl From<&crate::modifier::Color> for Color4f {
    fn from(c: &crate::modifier::Color) -> Self {
        Color4f::new(
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
            c.a as f32 / 255.0,
        )
    }
}

fn draw_background(canvas: &Canvas, rect: Rect, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_anti_alias(true);
    match shape {
        crate::modifier::Shape::Rectangle => { canvas.draw_rect(rect, &paint); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), &paint);
        }
        crate::modifier::Shape::Circle => {
            canvas.draw_circle((rect.center_x(), rect.center_y()), rect.width().min(rect.height()) / 2.0, &paint);
        }
    }
}

fn draw_border(canvas: &Canvas, x: f32, y: f32, w: f32, h: f32, width: f32, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width);
    paint.set_anti_alias(true);
    let inset = width / 2.0;
    let sr = Rect::new(x + inset, y + inset, x + w - inset, y + h - inset);
    match shape {
        crate::modifier::Shape::Rectangle => { canvas.draw_rect(sr, &paint); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(sr, (*corner_radius - inset).max(0.0), (*corner_radius - inset).max(0.0)), &paint);
        }
        crate::modifier::Shape::Circle => {
            canvas.draw_circle((sr.center_x(), sr.center_y()), sr.width().min(sr.height()) / 2.0, &paint);
        }
    }
}

fn draw_focus(canvas: &Canvas, rect: Rect) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::new(0.3, 0.6, 1.0, 0.8), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(2.0);
    paint.set_anti_alias(true);
    canvas.draw_rect(rect, &paint);
}

fn draw_text(
    canvas: &Canvas,
    content: &str,
    font_size: f32,
    color: &crate::modifier::Color,
    font_weight: crate::ui::text::FontWeight,
    font_style: crate::ui::text::FontSlant,
    x: f32,
    y: f32,
    max_width: f32,
    max_lines: usize,
    align: crate::ui::TextAlign,
    overflow: crate::ui::TextOverflow,
) {
    let mut para_style = ParagraphStyle::new();

    // max_lines：限制行数
    if max_lines < usize::MAX {
        para_style.set_max_lines(max_lines);
    }

    // ellipsis overflow：超出时显示省略号
    if overflow == crate::ui::TextOverflow::Ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    // justify alignment：两端对齐需要 Skia 内部调整单词间距
    if align == crate::ui::TextAlign::Justify {
        para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify);
    }

    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
    // 设置字重和倾斜
    if font_weight != crate::ui::text::FontWeight::NORMAL || font_style != crate::ui::text::FontSlant::Upright {
        use skia_safe::FontStyle;
        use crate::ui::text::FontSlant;
        let slant = match font_style {
            FontSlant::Upright => skia_safe::font_style::Slant::Upright,
            FontSlant::Italic => skia_safe::font_style::Slant::Italic,
            FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
        };
        text_style.set_font_style(FontStyle::new(font_weight.value().into(), 5.into(), slant));
    }
    let fc = crate::font::get_font_collection();
    let mut builder = ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(content);
    let mut para = builder.build();
    para.layout(max_width);
    // 计算 x 偏移以支持 Center/Right/Justify 对齐
    let x_offset = match align {
        crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => x,
        crate::ui::TextAlign::Center => x + (max_width - para.max_intrinsic_width()).max(0.0) / 2.0,
        crate::ui::TextAlign::Right => x + (max_width - para.max_intrinsic_width()).max(0.0),
    };
    para.paint(canvas, (x_offset, y));
}

fn surface_snapshot(canvas: &Canvas, bounds: skia_safe::IRect) -> Option<skia_safe::Image> {
    let surface = unsafe { canvas.surface() }?;
    let mut surface = surface.clone();
    surface.image_snapshot_with_bounds(bounds)
}
