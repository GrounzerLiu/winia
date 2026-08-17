//! 渲染快照测试 — skia CPU 光栅化，无窗口、无 GPU。
//!
//! 验证 compose → layout → render 端到端正确性。所有断言均为语义级
//!（允许 ±10 颜色容差），不依赖像素精确匹配。

use winia::core::composer::{Composer, ComposeCtx};
use winia::layout::constraints::Constraints;
use winia::modifier::{Modifier, Color, Shape};
use winia::ui::{Text, Button, Column, Row, FloatingActionButton, FloatingActionButtonSize, Icon};
use winia::render;

// ── 辅助 ──

fn raster_surface(w: i32, h: i32) -> skia_safe::Surface {
    skia_safe::surfaces::raster_n32_premul((w, h)).expect("raster surface")
}

fn pixel(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8, u8) {
    let mut pixels = [0u8; 4];
    let info = skia_safe::ImageInfo::new(
        (1, 1), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None,
    );
    surface.read_pixels(&info, &mut pixels, 4, (x, y));
    (pixels[0], pixels[1], pixels[2], pixels[3])
}

fn color_close(c: (u8, u8, u8), expected: (u8, u8, u8), tolerance: u8) -> bool {
    let dr = if c.0 > expected.0 { c.0 - expected.0 } else { expected.0 - c.0 };
    let dg = if c.1 > expected.1 { c.1 - expected.1 } else { expected.1 - c.1 };
    let db = if c.2 > expected.2 { c.2 - expected.2 } else { expected.2 - c.2 };
    dr <= tolerance && dg <= tolerance && db <= tolerance
}

fn render_ui(w: f32, h: f32, ui: impl FnOnce(&mut ComposeCtx)) -> (skia_safe::Surface, Composer) {
    let mut composer = Composer::new();
    composer.compose(|ctx| ui(ctx));
    composer.layout(Constraints::new(0.0, w, 0.0, h));
    let mut surface = raster_surface(w as i32, h as i32);
    surface.canvas().clear(skia_safe::Color::WHITE);
    if let Some(root) = composer.layout_root() {
        if let Some(root_idx) = composer.layout_root_idx() {
            let nodes = composer.arena_nodes();
            render::render(nodes, root_idx, surface.canvas());
        }
    }
    (surface, composer)
}

// ═══════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════

#[test]
fn empty_canvas_is_white() {
    let (mut surface, _) = render_ui(100.0, 60.0, |_ctx| {});
    let (r, g, b, _) = pixel(&mut surface, 50, 30);
    assert!(color_close((r, g, b), (255, 255, 255), 5));
}

#[test]
fn solid_background_fills_rect() {
    // 40×30 红色矩形，不填满 100×60 surface
    let (mut surface, _) = render_ui(100.0, 60.0, winia::app_root!(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(40.0, 30.0).background(Color::RED, Shape::Rectangle));
        ctx.end_node();
    }));

    let (r, g, b, _) = pixel(&mut surface, 20, 15);
    assert!(color_close((r, g, b), (255, 0, 0), 10),
        "center should be red, got ({r},{g},{b})");

    // 右下角在矩形外，应保持白色
    let (r2, g2, b2, _) = pixel(&mut surface, 90, 50);
    assert!(color_close((r2, g2, b2), (255, 255, 255), 5),
        "outside should be white, got ({r2},{g2},{b2})");
}

#[test]
fn blue_rounded_rect() {
    let (mut surface, _) = render_ui(80.0, 40.0, winia::app_root!(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key,
            Modifier::new().size(80.0, 40.0).background(Color::BLUE, Shape::rounded(6.0)));
        ctx.end_node();
    }));

    let (r, g, b, _) = pixel(&mut surface, 40, 20);
    assert!(color_close((r, g, b), (0, 0, 255), 10));
}

#[test]
fn text_renders_something() {
    // 不测精确字形——只确保文字区域不是全白
    let (mut surface, _) = render_ui(200.0, 40.0, winia::app_root!(|ctx| {
        Text::new("Hello World").font_size(20.0).color(Color::BLACK).build(ctx);
    }));

    let mut dark_pixels = 0;
    for x in 10..150 {
        let (r, g, b, _) = pixel(&mut surface, x, 15);
        if r < 200 && g < 200 && b < 200 { dark_pixels += 1; }
    }
    assert!(dark_pixels > 5, "expected dark text pixels, found {dark_pixels}");
}

#[test]
fn column_stacks_vertically() {
    // Column: 上方 30px 红色 + 下方填满蓝色
    let (mut surface, _) = render_ui(200.0, 100.0, winia::app_root!(|ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(200.0, 30.0).background(Color::RED, Shape::Rectangle));
            ctx.end_node();
            let k2 = ctx.next_key();
            ctx.start_leaf(k2, Modifier::new().fill_max_width().fill_max_height().background(Color::BLUE, Shape::Rectangle));
            ctx.end_node();
        });
    }));

    // y=10 在红色区域
    let (r, _, _, _) = pixel(&mut surface, 100, 10);
    assert!(r > 200, "top region should be reddish, R={r}");

    // y=60 在蓝色区域
    let (_, _, b, _) = pixel(&mut surface, 100, 60);
    assert!(b > 200, "bottom region should be bluish, B={b}");
}

#[test]
fn row_arranges_horizontally() {
    // Row: 左 50px 红色 + 右 50px 蓝色
    let (mut surface, _) = render_ui(100.0, 40.0, winia::app_root!(|ctx| {
        Row::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(50.0, 40.0).background(Color::RED, Shape::Rectangle));
            ctx.end_node();
            let k2 = ctx.next_key();
            ctx.start_leaf(k2, Modifier::new().size(50.0, 40.0).background(Color::BLUE, Shape::Rectangle));
            ctx.end_node();
        });
    }));

    let (r, _, _, _) = pixel(&mut surface, 20, 20);
    assert!(r > 200, "left should be reddish, R={r}");

    let (_, _, b, _) = pixel(&mut surface, 70, 20);
    assert!(b > 200, "right should be bluish, B={b}");
}

#[test]
fn button_with_text_does_not_crash() {
    // 最基本的集成——确保不 panic
    let (_surface, _) = render_ui(200.0, 60.0, winia::app_root!(|ctx| {
        Button::new()
            .modifier(Modifier::new().size(120.0, 36.0).background(Color::BLUE, Shape::rounded(4.0)))
            .build(ctx, |ctx| {
                Text::new("Click").color(Color::WHITE).font_size(14.0).build(ctx);
            });
    }));
}

#[test]
fn floating_action_button_renders_rounded_shape_and_content() {
    let (mut surface, _) = render_ui(120.0, 120.0, winia::app_root!(|ctx| {
        FloatingActionButton::new().build(ctx, |ctx| {
            Icon::svg_path("M12 5v14M5 12h14")
                .size(FloatingActionButtonSize::Regular.icon_size())
                .build(ctx);
        });
    }));

    let theme = winia::ui::theme::ThemeColors::default_light();
    let center = pixel(&mut surface, 28, 28);
    assert!(color_close(
        (center.0, center.1, center.2),
        (theme.primary_container.r, theme.primary_container.g, theme.primary_container.b),
        12,
    ), "FAB center should use primary-container, got {center:?}");

    let corner = pixel(&mut surface, 0, 0);
    assert!(
        !color_close(
            (corner.0, corner.1, corner.2),
            (
                theme.primary_container.r,
                theme.primary_container.g,
                theme.primary_container.b
            ),
            12
        ),
        "rounded corner should not be filled by the FAB container, got {corner:?}"
    );

    let mut non_white = 0;
    for y in 12..44 {
        for x in 12..44 {
            let (r, g, b, _) = pixel(&mut surface, x, y);
            if r < 240 || g < 240 || b < 240 { non_white += 1; }
        }
    }
    assert!(non_white > 20, "FAB should contain rendered content and container pixels");
}

#[test]
fn floating_action_button_medium_uses_rounded_container() {
    let (mut surface, _) = render_ui(140.0, 120.0, winia::app_root!(|ctx| {
        FloatingActionButton::medium().build(ctx, |_ctx| {});
    }));

    let center = pixel(&mut surface, 40, 40);
    assert!(center.0 < 255 || center.1 < 255 || center.2 < 255);
    let outer_corner = pixel(&mut surface, 0, 0);
    assert!(color_close((outer_corner.0, outer_corner.1, outer_corner.2), (255, 255, 255), 12),
        "rounded FAB corner should remain background, got {outer_corner:?}");
}

#[test]
fn disabled_floating_action_button_still_renders_without_interaction() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        FloatingActionButton::new()
            .enabled(false)
            .on_click(|| panic!("disabled FAB must not invoke callback"))
            .build(ctx, |_ctx| {});
    });
    let root = composer.layout_root().expect("FAB root");
    assert_eq!(root.children.len(), 0);
    assert!(root.modifier.clickable_interaction().is_none());
    assert!(root.modifier.focusable_interaction().is_none());
    assert!(root.modifier.ripple_interaction().is_none());
}

#[test]
fn nested_layout_does_not_crash() {
    let (_surface, _) = render_ui(300.0, 80.0, winia::app_root!(|ctx| {
        Row::new().build(ctx, |ctx| {
            Column::new().modifier(Modifier::new().size(100.0, 80.0).background(Color::RED, Shape::Rectangle))
                .build(ctx, |_ctx| {});
            Column::new().modifier(Modifier::new().size(100.0, 80.0).background(Color::BLUE, Shape::Rectangle))
                .build(ctx, |_ctx| {});
            Column::new().modifier(Modifier::new().fill_max_width().fill_max_height().background(Color::GREEN, Shape::Rectangle))
                .build(ctx, |_ctx| {});
        });
    }));
}

#[test]
fn smoke_render_counter_ui() {
    // 类似 counter 示例的完整 UI 结构
    use winia::prelude::*;

    let (_surface, _) = render_ui(400.0, 500.0, winia::app_root!(|ctx| {
        let count = ctx.remember(|| 0i32);

        Column::new().modifier(Modifier::new().padding(16.0)).build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get())).font_size(24.0).build(ctx);

            Button::new()
                .on_click({ let c = count.clone(); move || c.update(|v| *v += 1) })
                .modifier(Modifier::new().size(200.0, 36.0).background(Color::BLUE, Shape::rounded(4.0)))
                .build(ctx, |ctx| {
                    Text::new("+1").color(Color::WHITE).font_size(14.0).build(ctx);
                });

            // 滚动区域
            let scroll = ctx.remember(|| ScrollState::new()).get();
            Column::new()
                .modifier(Modifier::new().size(200.0, 100.0).vertical_scroll(scroll))
                .build(ctx, |ctx| {
                    for i in 0..5 {
                        Row::new()
                            .modifier(Modifier::new().size(200.0, 20.0).background(
                                Color::from_argb(255, 240, 240, 240), Shape::Rectangle))
                            .build(ctx, |ctx| {
                                Text::new(format!("Line {}", i)).font_size(12.0).build(ctx);
                            });
                    }
                });
        });
    }));
}
