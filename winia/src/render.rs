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
    FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle,
};
use std::cell::RefCell;

thread_local! {
    static FONT_COLLECTION: RefCell<Option<FontCollection>> = const { RefCell::new(None) };
}

fn get_font_collection() -> FontCollection {
    FONT_COLLECTION.with(|fc| {
        if fc.borrow().is_none() {
            let mut collection = FontCollection::new();
            collection.set_default_font_manager(skia_safe::FontMgr::default(), None);
            *fc.borrow_mut() = Some(collection);
        }
        fc.borrow().as_ref().unwrap().clone()
    })
}

// ── 入口 ──

pub fn render(root: &LayoutNode, canvas: &Canvas) {
    let mut backdrop_regions = Vec::new();
    // Phase 1: 非背景模糊内容 + 收集模糊区域
    render_pass1(root, canvas, 0.0, 0.0, &mut backdrop_regions);
    // Phase 2: 背景模糊
    if !backdrop_regions.is_empty() {
        render_backdrop_blur(root, canvas, &backdrop_regions);
    }
}

// ── Phase 1: 正常渲染（非 BackdropBlur 节点）──

fn render_pass1(
    node: &LayoutNode,
    canvas: &Canvas,
    parent_x: f32, parent_y: f32,
    backdrop_regions: &mut Vec<(f32, f32, f32, f32, f32, u64)>,
) {
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;
    if w <= 0.0 || h <= 0.0 { return; }

    let rect = Rect::new(x, y, x + w, y + h);
    let mut blur_radius: Option<f32> = None;
    let mut is_backdrop = false;
    let mut text: Option<(&str, f32, &crate::modifier::Color, usize, crate::ui::TextAlign, crate::ui::TextOverflow)> = None;
    let mut scroll_offset_v: Option<f32> = None;
    let mut scroll_offset_h: Option<f32> = None;

    for el in node.modifier.elements() {
        match el {
            ModifierElement::Background { color, shape } => {
                draw_background(canvas, rect, color, shape);
            }
            ModifierElement::Border { width, color, shape } => {
                draw_border(canvas, x, y, w, h, *width, color, shape);
            }
            ModifierElement::Blur { radius } => {
                blur_radius = Some(*radius);
            }
            ModifierElement::BackdropBlur { radius } => {
                is_backdrop = true;
                backdrop_regions.push((x, y, w, h, *radius, node.id));
            }
            ModifierElement::TextContent { content, font_size, color, max_lines, align, overflow } => {
                text = Some((content, *font_size, color, *max_lines, *align, *overflow));
            }
            ModifierElement::VerticalScroll { state } => {
                scroll_offset_v = Some(state.get());
            }
            ModifierElement::HorizontalScroll { state } => {
                scroll_offset_h = Some(state.get());
            }
            _ => {}
        }
    }

    // 内容模糊：saveLayer
    if let Some(r) = blur_radius {
        let mut paint = Paint::default();
        paint.set_image_filter(image_filters::blur((r, r), skia_safe::TileMode::Clamp, None, None));
        let rec = skia_safe::canvas::SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }

    if let Some((content, font_size, color, max_lines, align, overflow)) = text {
        draw_text(canvas, content, font_size, color, x, y, w, max_lines, align, overflow);
    }
    if node.focused {
        draw_focus(canvas, rect);
    }

    // Scroll clip + translate
    let mut scrolled = false;
    if scroll_offset_v.is_some() || scroll_offset_h.is_some() {
        // clip 用 visible 尺寸（从 modifier Size 中取），不是 content 尺寸
        let cw = node.modifier.elements().iter().find_map(|el| match el {
            ModifierElement::Size { width: Dimension::Fixed(w), .. } => Some(*w),
            _ => None,
        }).unwrap_or(w);
        let ch = node.modifier.elements().iter().find_map(|el| match el {
            ModifierElement::Size { height: Dimension::Fixed(h), .. } => Some(*h),
            _ => None,
        }).unwrap_or(h);
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
            render_pass1(child, canvas, x, y, backdrop_regions);
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
    root: &LayoutNode,
    canvas: &Canvas,
    regions: &[(f32, f32, f32, f32, f32, u64)],
) {
    // 取整张 surface snapshot（只回读一次）
    let mut snap_bounds: Option<Rect> = None;
    for &(x, y, w, h, r, _node_id) in regions {
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
    for &(x, y, w, h, radius, node_id) in regions {
        if let Some(ref snap) = snapshot {
            let mut paint = Paint::default();
            paint.set_image_filter(image_filters::blur((radius, radius), skia_safe::TileMode::Clamp, None, None));
            let m = radius * 2.0;
            let src = Rect::new((x - m).max(0.0), (y - m).max(0.0), x + w + m, y + h + m);
            canvas.draw_image_rect(snap, None, &src, &paint);
        }
        // 画回模糊节点自己的子节点
        if let Some(backdrop_node) = find_node_by_id(root, node_id) {
            for child in &backdrop_node.children {
                render_pass1_simple(child, canvas, x, y);
            }
        }
    }
}

fn render_pass1_simple(node: &LayoutNode, canvas: &Canvas, px: f32, py: f32) {
    let x = px + node.position.x;
    let y = py + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;
    if w <= 0.0 || h <= 0.0 { return; }
    let rect = Rect::new(x, y, x + w, y + h);
    let mut text: Option<(&str, f32, &crate::modifier::Color, usize, crate::ui::TextAlign, crate::ui::TextOverflow)> = None;
    for el in node.modifier.elements() {
        match el {
            ModifierElement::Background { color, shape } => draw_background(canvas, rect, color, shape),
            ModifierElement::Border { width, color, shape } => draw_border(canvas, x, y, w, h, *width, color, shape),
            ModifierElement::TextContent { content, font_size, color, max_lines, align, overflow } => { text = Some((content, *font_size, color, *max_lines, *align, *overflow)); }
            _ => {}
        }
    }
    if let Some((c, fs, cl, ml, al, ov)) = text { draw_text(canvas, c, fs, cl, x, y, w, ml, al, ov); }
    if node.focused { draw_focus(canvas, rect); }
    for child in &node.children { render_pass1_simple(child, canvas, x, y); }
}

// ── 辅助函数 ──

fn draw_background(canvas: &Canvas, rect: Rect, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::new(
        color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0, color.a as f32 / 255.0,
    ), None);
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
    paint.set_color4f(Color4f::new(
        color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0, color.a as f32 / 255.0,
    ), None);
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

fn draw_text(canvas: &Canvas, content: &str, font_size: f32, color: &crate::modifier::Color, x: f32, y: f32, max_width: f32, _max_lines: usize, align: crate::ui::TextAlign, _overflow: crate::ui::TextOverflow) {
    let para_style = ParagraphStyle::new();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
    let fc = get_font_collection();
    let mut builder = ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(content);
    let mut para = builder.build();
    para.layout(max_width);
    // 计算 x 偏移以支持 Center/Right 对齐
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

/// 通过唯一 ID 精确查找节点（替代浮点坐标近似匹配）
fn find_node_by_id(node: &LayoutNode, id: u64) -> Option<&LayoutNode> {
    if node.id == id {
        return Some(node);
    }
    for child in &node.children {
        if let Some(n) = find_node_by_id(child, id) {
            return Some(n);
        }
    }
    None
}
