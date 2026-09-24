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

/// `Modifier::z_index` decides which sibling covers which: the raised child wins even when it is
/// composed FIRST, and without any z the later child stays on top (tree order — the fast path the
/// renderer keeps for every node that never sets a z). The hit-test side of the same rule is pinned in
/// `layout::node`'s `test_hit_test_follows_z_index`.
#[test]
fn z_index_reorders_sibling_painting() {
    use winia::prelude::*;

    let paint_two_boxes = |raise_first: bool| -> (u8, u8, u8) {
        let (mut surface, _) = render_ui(60.0, 60.0, winia::app_root!(move |ctx| {
            Stack::new().build(ctx, |ctx| {
                // Composed first, so it is the bottom one by tree order.
                let mut first = Modifier::new().size(60.0, 60.0).background(Color::RED, Shape::Rectangle);
                if raise_first {
                    first = first.z_index(1.0);
                }
                Column::new().modifier(first).build(ctx, |_ctx| {});
                // Composed second: on top unless the first one was raised.
                Column::new()
                    .modifier(Modifier::new().size(60.0, 60.0).background(Color::BLUE, Shape::Rectangle))
                    .build(ctx, |_ctx| {});
            });
        }));
        let p = pixel(&mut surface, 30, 30);
        (p.0, p.1, p.2)
    };

    assert!(
        color_close(paint_two_boxes(false), (0, 0, 255), 10),
        "without z the later sibling paints on top"
    );
    assert!(
        color_close(paint_two_boxes(true), (255, 0, 0), 10),
        "with z_index the raised (earlier) sibling paints on top"
    );
}

// ── Brushes / gradients ──
//
// These read PIXELS, which is the only way to test a gradient: nothing in the layout tree or the
// semantics tree names a color. Each test states the gradient's direction and then asserts what the
// renderer actually put at the two ends and the middle.

use winia::brush::{Brush, BrushTile};

const RED: (u8, u8, u8) = (255, 0, 0);
const BLUE: (u8, u8, u8) = (0, 0, 255);

fn red() -> Color {
    Color::from_argb(255, 255, 0, 0)
}

fn blue() -> Color {
    Color::from_argb(255, 0, 0, 255)
}

/// Draw one `w`x`h` node filled with `brush` — a value or a closure, as the modifier takes.
fn brush_scene(brush: impl Into<winia::brush::BrushSource>, w: f32, h: f32) -> skia_safe::Surface {
    let brush = brush.into();
    let (mut surface, _) = render_ui(w, h, winia::app_root!(move |ctx| {
        use winia::ui::layout_components::Column;
        Column::new()
            .modifier(Modifier::new().size(w, h).background_brush(brush, Shape::Rectangle))
            .build(ctx, |_| {});
    }));
    surface
}

/// The RGB at a point.
fn rgb(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8) {
    let (r, g, b, _) = pixel(surface, x, y);
    (r, g, b)
}

#[test]
fn a_linear_gradient_runs_left_to_right_by_default() {
    let mut surface = brush_scene(Brush::linear_gradient([red(), blue()]), 100.0, 100.0);

    // Sampling at x=2 and x=97 stays clear of the antialiased first and last column.
    let left = rgb(&mut surface, 2, 50);
    let middle = rgb(&mut surface, 50, 50);
    let right = rgb(&mut surface, 97, 50);

    assert!(color_close(left, RED, 12), "the left end is the first stop, got {left:?}");
    assert!(color_close(right, BLUE, 12), "the right end is the last stop, got {right:?}");
    assert!(
        left.0 > middle.0 && middle.0 > right.0 && left.2 < middle.2 && middle.2 < right.2,
        "red falls and blue rises across the run: {left:?} {middle:?} {right:?}"
    );
    assert!(color_close(middle, (128, 0, 128), 30), "the middle is the even blend, got {middle:?}");

    // A horizontal gradient is constant down its length.
    assert_eq!(rgb(&mut surface, 50, 5), rgb(&mut surface, 50, 95));
}

#[test]
fn the_orientation_helpers_run_the_other_ways() {
    let mut surface = brush_scene(Brush::linear_gradient([red(), blue()]).vertical(), 100.0, 100.0);
    assert!(color_close(rgb(&mut surface, 50, 2), RED, 12), "vertical: the top is the first stop");
    assert!(color_close(rgb(&mut surface, 50, 97), BLUE, 12), "vertical: the bottom is the last stop");

    let mut surface = brush_scene(Brush::linear_gradient([red(), blue()]).diagonal(), 100.0, 100.0);
    assert!(color_close(rgb(&mut surface, 3, 3), RED, 24), "diagonal: top-left is the first stop");
    assert!(color_close(rgb(&mut surface, 96, 96), BLUE, 24), "diagonal: bottom-right is the last stop");
}

#[test]
fn a_radial_gradient_runs_outward_from_the_centre() {
    // The centre is the node's MIDDLE, whatever the aspect ratio: a radial brush's `from` is a centre,
    // not the linear brush's start point. (Getting that wrong put the whole pattern around the left
    // edge — measured, then fixed.)
    let mut surface = brush_scene(Brush::radial_gradient([red(), blue()]), 200.0, 100.0);

    assert!(color_close(rgb(&mut surface, 100, 50), RED, 12), "the centre is the first stop");
    // Half the SHORTER side is 50, so 25px out is halfway along the ramp in every direction.
    for (label, sample) in [
        ("east", rgb(&mut surface, 125, 50)),
        ("west", rgb(&mut surface, 75, 50)),
        ("north", rgb(&mut surface, 100, 25)),
        ("south", rgb(&mut surface, 100, 75)),
    ] {
        assert!(
            color_close(sample, (153, 0, 102), 30),
            "{label} at 25px of a 50px radius is halfway along the ramp, got {sample:?}"
        );
    }
    // And the far end of the radius is the last stop.
    let edge = rgb(&mut surface, 100, 97);
    assert!(edge.2 > 200 && edge.0 < 40, "the radius reaches the shorter side's edge, got {edge:?}");
}

#[test]
fn a_radial_radius_is_a_fraction_of_the_shorter_side() {
    // On a wide node a radius of 0.5 reaches the top and bottom edges, not the far left/right ones —
    // which is what lets a "spotlight" gradient fill a card at any aspect ratio.
    let mut surface = brush_scene(Brush::radial_gradient([red(), blue()]), 200.0, 100.0);
    let top = rgb(&mut surface, 100, 3);
    assert!(top.2 > 190, "the top edge is at the radius' end, got {top:?}");

    // A radius of 1.5 of the shorter side has not been reached at the far left yet.
    let mut surface = brush_scene(Brush::radial_gradient([red(), blue()]).radius(1.5), 200.0, 100.0);
    let left = rgb(&mut surface, 3, 50);
    assert!(
        left.2 < 200 && left.0 > 60,
        "a radius of 1.5 of the shorter side is still not reached at the far left, got {left:?}"
    );
}

#[test]
fn a_sweep_gradient_wraps_clockwise_from_three_oclock() {
    // Skia's convention (and CSS conic-gradient's): 0 degrees at the +x axis, clockwise, one full
    // turn. With two stops at 0.0 and 0.5 the run reaches the second color at 180 degrees (9 o'clock)
    // and holds from there back around to 3 o'clock.
    let mut surface = brush_scene(
        Brush::sweep_gradient([red(), blue()]).positions([0.0, 0.5]),
        100.0,
        100.0,
    );

    assert!(color_close(rgb(&mut surface, 97, 50), RED, 12), "the sweep starts at 3 o'clock");
    assert!(color_close(rgb(&mut surface, 50, 97), (128, 0, 128), 30), "6 o'clock is halfway");
    assert!(color_close(rgb(&mut surface, 3, 50), BLUE, 12), "9 o'clock is the last stop");
    assert!(color_close(rgb(&mut surface, 50, 3), BLUE, 12), "and 12 o'clock holds it");
}

#[test]
fn explicit_stops_place_the_colors() {
    // Everything before the first stop holds the first color, so at the middle (0.5) a gradient whose
    // stops start at 0.8 is still red.
    let mut surface = brush_scene(Brush::linear_gradient([red(), blue()]).positions([0.8, 1.0]), 100.0, 100.0);
    assert!(color_close(rgb(&mut surface, 40, 50), RED, 6), "before the first stop the color holds");

    // And a gradient that finishes at 0.2 is already blue past it.
    let mut surface = brush_scene(Brush::linear_gradient([red(), blue()]).positions([0.0, 0.2]), 100.0, 100.0);
    assert!(color_close(rgb(&mut surface, 60, 50), BLUE, 6), "past the last stop the color holds");
}

#[test]
fn a_gradient_is_clipped_to_the_shape() {
    let (mut surface, _) = render_ui(100.0, 100.0, winia::app_root!(move |ctx| {
        use winia::ui::layout_components::Column;
        Column::new()
            .modifier(Modifier::new().size(100.0, 100.0).background_brush(
                Brush::linear_gradient([Color::from_argb(255, 255, 0, 0), Color::from_argb(255, 0, 0, 255)]),
                Shape::RoundedRect { corner_radius: 30.0 },
            ))
            .build(ctx, |_| {});
    }));
    let corner = rgb(&mut surface, 2, 2);
    assert!(
        corner.0 > 200 && corner.1 > 200 && corner.2 > 200,
        "the corner outside a 30dp radius stays background, got {corner:?}"
    );
    let inside = rgb(&mut surface, 50, 50);
    assert!(inside.0 > 0 || inside.2 > 0, "and the fill is clipped to the shape, not erased");
}

#[test]
fn a_tile_mode_repeats_or_mirrors_past_the_gradient() {
    // A tile mode only shows where the gradient's SPAN ends and the node continues — and the span is
    // set by `from_to`, not by the stop positions: the span maps onto the node, so a default brush
    // already fills it and Repeat/Mirror have nothing left to tile. (Measured: with stops up to 0.5
    // the ramp's second half stayed blue under every mode, so the stop positions were the wrong lever.)
    let ramps = |tile: BrushTile, x: i32| -> u8 {
        let (mut surface, _) = render_ui(100.0, 10.0, winia::app_root!(move |ctx| {
            use winia::ui::layout_components::Column;
            Column::new()
                .modifier(Modifier::new().size(100.0, 10.0).background_brush(
                    Brush::linear_gradient([Color::from_argb(255, 255, 0, 0), Color::from_argb(255, 0, 0, 255)])
                        .from_to((0.0, 0.5), (0.5, 0.5))
                        .tile(tile),
                    Shape::Rectangle,
                ))
                .build(ctx, |_| {});
        }));
        rgb(&mut surface, x, 5).0
    };

    // Inside the span all three are the same ramp: red at its start, most of the way to blue by its
    // end. (The span's exact end is where they part — Repeat restarts there, Mirror turns around, and
    // only Clamp holds the last color — so the samples stay inside it.)
    for tile in [BrushTile::Clamp, BrushTile::Repeat, BrushTile::Mirror] {
        assert!(ramps(tile, 0) > 240, "every mode ramps from red at the span's start");
        let near_end = ramps(tile, 40);
        assert!(near_end < 70, "and the ramp has run most of its length by the span's end, got {near_end}");
    }

    // Past the end, the three modes differ — which is the whole point of the setting.
    // One fifth of the way past the span's end is enough for all three to have diverged: Clamp is at
    // the last color, Mirror has come a fifth of the way back toward red, and Repeat is a fifth into
    // running the ramp again.
    let (clamp, repeat, mirror) = (
        ramps(BrushTile::Clamp, 60),
        ramps(BrushTile::Repeat, 60),
        ramps(BrushTile::Mirror, 60),
    );
    assert!(clamp < 20, "Clamp holds the last color, got {clamp}");
    assert!(mirror > 30 && mirror < 90, "Mirror runs it backwards, got {mirror}");
    assert!(repeat > 180, "Repeat runs the ramp again, got {repeat}");
    assert!(
        repeat > mirror + 60 && mirror > clamp + 10,
        "the three are distinct: clamp={clamp} mirror={mirror} repeat={repeat}"
    );
}

#[test]
fn a_brush_from_a_closure_is_read_at_paint_time() {
    // The animated-brush hook: `background_brush` takes a closure, so a gradient can follow state.
    let saw_closure = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = saw_closure.clone();
    let brush = move || {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        Brush::linear_gradient([Color::from_argb(255, 255, 0, 0), Color::from_argb(255, 0, 0, 255)])
    };
    let mut surface = brush_scene(brush, 100.0, 100.0);
    assert!(color_close(rgb(&mut surface, 2, 50), RED, 12), "the closure's brush painted");
    assert!(
        saw_closure.load(std::sync::atomic::Ordering::Relaxed),
        "and it ran at paint time, not at build time"
    );
}

#[test]
fn a_degenerate_brush_paints_nothing_instead_of_panicking() {
    // No colors, and a zero-length gradient: both are caller mistakes, and both must leave the surface
    // alone rather than panic inside a render pass.
    let mut surface = brush_scene(Brush::linear_gradient(Vec::new()), 100.0, 100.0);
    assert!(rgb(&mut surface, 2, 50).0 > 200, "an empty gradient paints nothing");

    let mut surface = brush_scene(
        Brush::linear_gradient([red(), blue()]).from_to((0.5, 0.5), (0.5, 0.5)),
        100.0,
        100.0,
    );
    assert!(rgb(&mut surface, 2, 50).0 > 200, "a zero-length gradient paints nothing");
}

#[test]
fn a_solid_brush_is_the_same_as_a_solid_background() {
    let mut surface = brush_scene(Brush::solid(red()), 100.0, 100.0);
    assert_eq!(rgb(&mut surface, 50, 50), RED, "a solid brush fills with its color");
}
