//! 渲染管线 — 遍历 LayoutNode 树绘制到 Skia Canvas

use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use skia_safe::{Canvas, Color4f, Paint, RRect, Rect};
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

pub fn render(root: &LayoutNode, canvas: &Canvas) {
    render_node(root, canvas, 0.0, 0.0);
}

fn render_node(
    node: &LayoutNode,
    canvas: &Canvas,
    parent_x: f32,
    parent_y: f32,
) {
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;

    if w <= 0.0 || h <= 0.0 {
        return;
    }

    let rect = Rect::new(x, y, x + w, y + h);
    let mut text_to_draw: Option<(&str, f32, &crate::modifier::Color)> = None;

    for el in node.modifier.elements() {
        match el {
            ModifierElement::Background { color, shape } => {
                let mut paint = Paint::default();
                paint.set_color4f(Color4f::new(
                    color.r as f32 / 255.0, color.g as f32 / 255.0,
                    color.b as f32 / 255.0, color.a as f32 / 255.0,
                ), None);
                paint.set_anti_alias(true);
                match shape {
                    crate::modifier::Shape::Rectangle => { canvas.draw_rect(rect, &paint); }
                    crate::modifier::Shape::RoundedRect { corner_radius } => {
                        canvas.draw_rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), &paint);
                    }
                    crate::modifier::Shape::Circle => {
                        let r = rect.width().min(rect.height()) / 2.0;
                        canvas.draw_circle((rect.center_x(), rect.center_y()), r, &paint);
                    }
                }
            }
            ModifierElement::Border { width, color, shape } => {
                let mut paint = Paint::default();
                paint.set_color4f(Color4f::new(
                    color.r as f32 / 255.0, color.g as f32 / 255.0,
                    color.b as f32 / 255.0, color.a as f32 / 255.0,
                ), None);
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(*width);
                paint.set_anti_alias(true);
                let inset = width / 2.0;
                let sr = Rect::new(x + inset, y + inset, x + w - inset, y + h - inset);
                match shape {
                    crate::modifier::Shape::Rectangle => { canvas.draw_rect(sr, &paint); }
                    crate::modifier::Shape::RoundedRect { corner_radius } => {
                        let r = (corner_radius - inset).max(0.0);
                        canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
                    }
                    crate::modifier::Shape::Circle => {
                        let r = sr.width().min(sr.height()) / 2.0;
                        canvas.draw_circle((sr.center_x(), sr.center_y()), r, &paint);
                    }
                }
            }
            ModifierElement::TextContent { content, font_size, color } => {
                text_to_draw = Some((content, *font_size, color));
            }
            _ => {}
        }
    }

    if let Some((content, font_size, color)) = text_to_draw {
        draw_text(canvas, content, font_size, color, x, y, w);
    }

    // 焦点指示框
    if node.focused {
        let mut paint = Paint::default();
        paint.set_color4f(Color4f::new(0.3, 0.6, 1.0, 0.8), None);
        paint.set_style(skia_safe::paint::Style::Stroke);
        paint.set_stroke_width(2.0);
        paint.set_anti_alias(true);
        canvas.draw_rect(rect, &paint);
    }

    for child in &node.children {
        render_node(child, canvas, x, y);
    }
}

fn draw_text(canvas: &Canvas, content: &str, font_size: f32, color: &crate::modifier::Color, x: f32, y: f32, max_width: f32) {
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
    para.paint(canvas, (x, y));
}
