    use super::*;
    use super::backdrop::*;
    use super::helpers::*;
    use super::paint::*;
    use super::pass::*;
    use super::text_field::*;

    /// The regression this guards: a container MERGES its children's alignment lines
    /// (`layout/node.rs:3346-3367`), so reading `minimum_interactive_left/top` off the node while painting
    /// an ancestor inset that ancestor's own background. The inset lives on the min-sized node now, and the
    /// container above it must paint its full box.
    #[test]
    fn a_container_above_a_minimum_interactive_child_does_not_inset_itself() {
        use crate::components::icon_button::IconButton;
        use crate::modifier::Modifier;
        use crate::runtime::composer::{Composer, GroupStatus};
        use crate::theme::{ThemeColors, WiniaTheme};

        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let key = ctx.next_key();
                match ctx.start_restartable_group(
                    key,
                    Modifier::new()
                        .size(200.0, 100.0)
                        .background(theme.primary, crate::graphics::Shape::Rectangle),
                    crate::layout::BoxLayout::new(),
                ) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        IconButton::filled().on_click(|| {}).build(ctx, |_ctx| {});
                    }
                }
                ctx.end_restartable_group();
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 100.0));
        let root = composer.layout_root_idx().expect("root");
        let mut surface = skia_safe::surfaces::raster_n32_premul((200, 100)).expect("surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        render(composer.arena_nodes(), root, surface.canvas());

        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // A row below the 48 dp child (which sits at the origin), so the whole width is the container.
        let row = &px[70 * 200..71 * 200];
        let painted = |x: usize| row[x][0] < 250 || row[x][1] < 250 || row[x][2] < 250;
        assert!(
            painted(0) && painted(199),
            "the container must paint its full 200 dp box: the child's minimum-interactive inset belongs \
             to the child, not to this node (left={}, right={})",
            painted(0),
            painted(199)
        );
    }

    /// The outline belongs to the container, not to the node that `minimum_interactive_size` grew to the
    /// 48 dp touch target: a Small Outlined IconButton paints a 40 dp box, stroke included. Before the fix
    /// the stroke followed the node and measured 48, which is what made it look oversized.
    #[test]
    fn an_outlined_icon_button_paints_its_outline_at_the_container_size() {
        use crate::components::icon_button::IconButton;
        use crate::runtime::composer::Composer;
        use crate::theme::{ThemeColors, WiniaTheme};

        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconButton::outlined().on_click(|| {}).build(ctx, |_ctx| {});
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 120.0, 0.0, 120.0));
        let root = composer.layout_root_idx().expect("root");
        let mut surface = skia_safe::surfaces::raster_n32_premul((120, 120)).expect("surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        render(composer.arena_nodes(), root, surface.canvas());

        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // Scan the vertical middle: a transparent container leaves only the two stroke edges painted.
        let row = &px[24 * 120..25 * 120];
        let painted: Vec<usize> = (0..120)
            .filter(|x| row[*x][0] < 250 || row[*x][1] < 250 || row[*x][2] < 250)
            .collect();
        let (left, right) = (
            *painted.first().expect("the outline should paint something"),
            *painted.last().unwrap(),
        );
        assert_eq!(
            (right - left + 1) as f32,
            40.0,
            "a Small Outlined IconButton's outline spans its 40 dp container, not the 48 dp node \
             (painted columns {left}..={right})"
        );
    }

    /// The cross-fade wrapper must not change the glyph's colour: it only adds a per-generation alpha
    /// layer, and a resting generation sits at alpha 1. Painting the same button with and without
    /// `Crossfade` and comparing the glyph's centre pixel is what catches a colour the layer machinery
    /// gets wrong. `Standard` is the style to use here because its container is transparent, so the pixel
    /// read is the glyph over the white background rather than over a container that masks the difference.
    #[test]
    fn the_crossfade_wrapper_does_not_change_the_glyph_colour() {
        use crate::components::icon_toggle_button::IconToggleButton;
        use crate::components::Crossfade;
        use crate::runtime::composer::Composer;
        use crate::runtime::state::State;
        use crate::theme::{ThemeColors, WiniaTheme};

        const SQUARE: &str = "M0 0h24v24H0z";

        let theme = ThemeColors::light_from_seed(0x6750A4);
        let sample = |wrapped: bool| -> [u8; 4] {
            let state = State::new(false);
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(state.get())
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            if wrapped {
                                Crossfade::new(state.clone()).build(ctx, |ctx, _on| {
                                    crate::components::icon::Icon::svg_path(SQUARE).build(ctx);
                                });
                            } else {
                                crate::components::icon::Icon::svg_path(SQUARE).build(ctx);
                            }
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 64.0, 0.0, 64.0));
            let root = composer.layout_root_idx().expect("root");
            let mut surface = skia_safe::surfaces::raster_n32_premul((64, 64)).expect("surface");
            surface.canvas().clear(skia_safe::Color::WHITE);
            render(composer.arena_nodes(), root, surface.canvas());
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            px[24 * 64 + 24]
        };

        let plain = sample(false);
        let wrapped = sample(true);
        assert_ne!(plain, [255, 255, 255, 255], "the glyph should paint at all");
        assert_eq!(
            wrapped, plain,
            "wrapping the glyph in a Crossfade must not change its colour"
        );
    }

    #[test]
    fn tf_aux_text_paints() {
        // label/支持文本绘制路径：build_plain_paragraph + layout + paint
        let mut surface = skia_safe::surfaces::raster_n32_premul((200, 40)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        draw_text_field_aux_text(
            canvas,
            "Name",
            12.0,
            crate::text::FontWeight::NORMAL,
            crate::text::FontSlant::Upright,
            0.0,
            None,
            &crate::graphics::Color::from_argb(255, 255, 0, 0),
            (10.0, 10.0),
            100.0,
        );
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // 文本区域应出现非白像素（红色文本）
        let mut found = 0;
        for y in 10..30 {
            for x in 10..60 {
                if px[y * 200 + x][2] > 200 { // R 通道（BGRA）
                    found += 1;
                }
            }
        }
        assert!(found > 20, "aux 文本必须可见（红色像素 {found} 个）");
    }

    #[test]
    fn fit_rect_preserves_aspect_and_centers() {
        let r = fit_rect(skia_safe::Rect::new(0.0, 0.0, 100.0, 50.0), 24.0, 24.0);
        assert_eq!((r.width(), r.height()), (50.0, 50.0), "等比缩放到短边");
        assert_eq!((r.left, r.top), (25.0, 0.0), "水平居中");
    }

    #[test]
    fn backdrop_snapshot_bounds_cover_3sigma() {
        // 快照物理像素边界必须覆盖 3σ 采样范围（blur sigma = radius，
        // 边缘像素采样 ±3r）——不足则 Clamp 重复像素产生边缘条纹。
        // 画布矩阵 = scale(1.5)（主画布 HiDPI 场景）。
        use skia_safe::Matrix;
        let mut m = Matrix::default();
        m.set_scale_x(1.5); m.set_scale_y(1.5);
        let (x, y, w, h, r) = (300.0f32, 250.0f32, 260.0f32, 170.0f32, 12.0f32);
        let corners = [
            m.map_point((x, y)), m.map_point((x + w, y)),
            m.map_point((x, y + h)), m.map_point((x + w, y + h)),
        ];
        let (sx0, sy0) = corners.iter().fold((f32::MAX, f32::MAX), |acc, p| (acc.0.min(p.x), acc.1.min(p.y)));
        let (sx1, sy1) = corners.iter().fold((f32::MIN, f32::MIN), |acc, p| (acc.0.max(p.x), acc.1.max(p.y)));
        let left = (sx0 - r * 3.0).floor() as i32;
        let top = (sy0 - r * 3.0).floor() as i32;
        let right = (sx1 + r * 3.0).ceil() as i32;
        let bottom = (sy1 + r * 3.0).ceil() as i32;
        assert!(sx0 - left as f32 >= r * 3.0 - 1.0, "左侧扩展覆盖 3σ");
        assert!(right as f32 - sx1 >= r * 3.0 - 1.0, "右侧扩展覆盖 3σ");
        assert!(sy0 - top as f32 >= r * 3.0 - 1.0, "顶部扩展覆盖 3σ");
        assert!(bottom as f32 - sy1 >= r * 3.0 - 1.0, "底部扩展覆盖 3σ");
    }

    #[test]
    fn backdrop_snapshot_irect_clamps_to_surface() {
        // 节点跨出顶/左边缘（如滚动内容滚出顶部）：快照边界必须 clamp
        // 到 surface——image_snapshot_with_bounds 返回图像锚定在相交原点，
        // 画回按 clamp 后原点锚定才不错位（旧实现 .max(0.0) 语义回归点）
        // 节点物理 AABB 在 (-50,-30)..(200,150)，surface 1350x960
        let b = backdrop_snapshot_irect(-50.0, -30.0, 200.0, 150.0, 36.0, 1350, 960).expect("有交集");
        assert_eq!((b.left, b.top), (0, 0), "顶/左边缘 clamp 到 0（相交原点锚定）");
        assert_eq!((b.right, b.bottom), (236, 186), "右侧/底部保持完整扩展");
        // 完全在 surface 外 → None
        assert!(backdrop_snapshot_irect(2000.0, 2000.0, 3000.0, 3000.0, 36.0, 1350, 960).is_none());
        // 右/下边缘 clamp
        let b2 = backdrop_snapshot_irect(1300.0, 900.0, 1360.0, 980.0, 36.0, 1350, 960).expect("有交集");
        assert_eq!((b2.right, b2.bottom), (1350, 960), "右/下边缘 clamp 到 surface 尺寸");
    }

    #[test]
    fn backdrop_draw_back_alignment_is_pixel_exact() {
        // 画回：快照原点经逆矩阵映射回画布逻辑坐标——再经画布矩阵后精确
        // 等于物理像素边界（像素网格对齐 → draw_image 零插值，移动不闪烁）
        use skia_safe::Matrix;
        let mut m = Matrix::default();
        m.set_scale_x(1.5); m.set_scale_y(1.5);
        let (x, y, r) = (300.0f32, 250.0f32, 12.0f32);
        let left = (x * 1.5 - r * 3.0).floor() as i32;
        let top = (y * 1.5 - r * 3.0).floor() as i32;
        let inv = m.invert().expect("invert");
        let origin = inv.map_point((left as f32, top as f32));
        let back = m.map_point((origin.x, origin.y));
        let (bdx, bdy) = (back.x, back.y);
        assert!((bdx - left as f32).abs() < 1e-3, "像素网格精确对齐：{bdx} vs {left}");
        assert!((bdy - top as f32).abs() < 1e-3, "像素网格精确对齐：{bdy} vs {top}");
    }

    #[test]
    fn backdrop_matrix_handles_ancestor_translate() {
        // 滚动容器/overlay 场景：画布含 translate（如 overlay 定位或 scroll
        // 偏移）——快照与画回必须基于矩阵映射的屏幕物理位置，裸逻辑坐标×sf
        // 会错位。验证：translate 下逆矩阵画回仍精确回到物理像素。
        use skia_safe::Matrix;
        let mut m = Matrix::default();
        m.set_scale_x(1.5); m.set_scale_y(1.5);
        m.set_translate_x(120.0); m.set_translate_y(80.0);
        let inv = m.invert().expect("invert");
        // 快照边界（矩阵映射后取整）
        let left = 450i32; let top = 300i32;
        let origin = inv.map_point((left as f32, top as f32));
        let back = m.map_point((origin.x, origin.y));
        let (bdx, bdy) = (back.x, back.y);
        assert!((bdx - left as f32).abs() < 1e-3, "translate 场景像素精确对齐：{bdx} vs {left}");
        assert!((bdy - top as f32).abs() < 1e-3, "translate 场景像素精确对齐：{bdy} vs {top}");
    }
    use crate::graphics::GraphicsLayerParams;


    /// The border's colour is a carrier too: a state-driven border repaints without recomposing.
    #[test]
    fn a_dynamic_border_colour_is_read_at_paint() {
        use crate::graphics::{Color, Shape};
        use crate::runtime::state::State;
        let state = State::new(Color::from_argb(255, 255, 0, 0));
        let modifier = crate::modifier::Modifier::new().border(4.0, &state, Shape::Rectangle);
        let el = &modifier.elements()[0];
        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let paint = |surface: &mut skia_safe::Surface| {
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::TRANSPARENT);
            let mut last_background = None;
            render_modifier_element(canvas, el, rect, &mut last_background, None);
        };
        // A 4 px stroke centred on the edge covers y in 0..4, so (50, 1) is fully inside it.
        let top_pixel = |surface: &mut skia_safe::Surface| -> [u8; 4] {
            let pm = surface.peek_pixels().expect("pixmap");
            let pixels: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            pixels[100 + 50]
        };

        // `raster_n32_premul` stores BGRA on this platform, so red reads as [0, 0, 255].
        paint(&mut surface);
        assert_eq!(top_pixel(&mut surface), [0, 0, 255, 255]);
        state.set(Color::from_argb(255, 0, 128, 0));
        paint(&mut surface);
        assert_eq!(top_pixel(&mut surface), [0, 128, 0, 255]);
    }

    /// A dynamic SHAPE is evaluated per paint: the same element paints a full rectangle and, after
    /// the state changes, a rounded box whose corner is transparent.
    #[test]
    fn a_dynamic_shape_changes_what_the_background_paints() {
        use crate::graphics::{Color, Shape};
        use crate::runtime::state::State;
        let shape_state = State::new(Shape::Rectangle);
        let color = Color::from_argb(255, 255, 0, 0);
        let modifier = crate::modifier::Modifier::new().background(color, &shape_state);
        let el = &modifier.elements()[0];
        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let paint = |surface: &mut skia_safe::Surface| {
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::TRANSPARENT);
            let mut last_background = None;
            render_modifier_element(canvas, el, rect, &mut last_background, None);
        };
        // (2,2) of a 100x100 box: inside a rectangle, outside a 40 px rounded corner.
        let corner_alpha = |surface: &mut skia_safe::Surface| -> u8 {
            let pm = surface.peek_pixels().expect("pixmap");
            let pixels: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            pixels[2 * 100 + 2][3]
        };

        paint(&mut surface);
        assert!(corner_alpha(&mut surface) > 0, "a rectangle background paints the corner");

        shape_state.set(Shape::rounded(40.0));
        paint(&mut surface);
        assert_eq!(
            corner_alpha(&mut surface),
            0,
            "a rounded shape leaves the corner transparent"
        );
    }


    #[test]
    fn an_inner_shadow_darkens_the_inside_edge() {
        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let mut bg = skia_safe::Paint::default();
        bg.set_color(skia_safe::Color::WHITE);
        canvas.draw_rect(rect, &bg);
        let color = crate::graphics::Color::from_argb(255, 0, 0, 0);
        draw_inner_shadow(
            canvas,
            rect,
            &crate::graphics::Shape::Rectangle,
            &color,
            6.0,
            0.0,
            0.0,
        );
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: usize, y: usize| px[y * 100 + x];
        let center = at(50, 50);
        let edge = at(1, 50);
        assert!(
            center[0] > 240 && center[1] > 240 && center[2] > 240,
            "the middle stays white: {center:?}"
        );
        assert!(
            edge[0] < 240 || edge[1] < 240 || edge[2] < 240,
            "the rim is darkened: {edge:?}"
        );
    }

    #[test]
    fn a_render_effect_blurs_the_layer_content() {
        let params = crate::graphics::GraphicsLayerParams::default();
        assert_eq!(params.compositing, crate::graphics::CompositingStrategy::Auto);
        assert!(params.render_effect.is_none());

        let modifier = crate::modifier::Modifier::new()
            .graphics_layer(|| crate::graphics::GraphicsLayerParams {
                render_effect: Some(crate::graphics::RenderEffect::Blur { radius: 8.0 }),
                ..Default::default()
            })
            .background(
                crate::graphics::Color::from_argb(255, 255, 0, 0),
                crate::graphics::Shape::Rectangle,
            );
        let mut node = LayoutNode::leaf(modifier);
        node.measured_size = crate::unit::Size::new(60.0, 60.0);
        let nodes = vec![node];
        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        render(&nodes, 0, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // BGRA: a hard red box has green 0 at x = 2; the blur blends it toward white.
        let g = px[30 * 100 + 2][1];
        assert!(
            g > 10 && g < 245,
            "the layer edge is a blend, not a hard boundary (g = {g})"
        );
    }

    #[test]
    fn clip_scrollable_container_keeps_cross_axis_slack_and_clips_the_main_axis() {
        let state = crate::modifier::ScrollState::new();
        let child = {
            let mut node = LayoutNode::leaf(crate::modifier::Modifier::new().background(
                crate::graphics::Color::from_argb(255, 255, 0, 0),
                crate::graphics::Shape::Rectangle,
            ));
            node.position = crate::unit::Offset::new(50.0, 10.0);
            node.measured_size = crate::unit::Size::new(200.0, 200.0);
            node
        };
        let mut parent = LayoutNode::new(
            crate::modifier::Modifier::new()
                .size(100.0, 100.0)
                .vertical_scroll(state)
                .clip_scrollable_container(crate::overscroll::Orientation::Vertical),
            Some(0),
        );
        parent.measured_size = crate::unit::Size::new(100.0, 100.0);
        parent.scroll_viewport_height = 100.0;
        parent.children = vec![1];
        let nodes = vec![parent, child];

        let mut surface = skia_safe::surfaces::raster_n32_premul((200, 200)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        render(&nodes, 0, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // The child paints 50..250 horizontally. The main (vertical) axis clip stops at y = 100;
        // the cross axis keeps 30 dp of slack, so x = 105 is still painted.
        assert!(
            px[50 * 200 + 105][1] < 128,
            "the cross axis keeps the 30 dp shadow slack"
        );
        assert!(
            px[105 * 200 + 50][1] > 200,
            "the main axis is still cut at the viewport"
        );
    }

    #[test]
    fn offscreen_compositing_forces_a_layer_by_itself() {
        let mut gl = crate::graphics::GraphicsLayerParams::default();
        assert!(!graphics_layer_needs_layer(&gl));
        gl.compositing = crate::graphics::CompositingStrategy::Offscreen;
        assert!(
            graphics_layer_needs_layer(&gl),
            "Offscreen composites even at alpha 1 and with no filter"
        );
        gl.compositing = crate::graphics::CompositingStrategy::ModulateAlpha;
        assert!(
            !graphics_layer_needs_layer(&gl),
            "ModulateAlpha has no per-draw equivalent in winia"
        );
        gl.alpha = 0.5;
        assert!(graphics_layer_needs_layer(&gl), "alpha still needs a layer");
    }

    #[test]
    fn test_gl_3d_matrix_none_without_3d() {
        let gl = GraphicsLayerParams::default();
        assert!(build_gl_3d_matrix(&gl, 200.0, 160.0).is_none(), "无 3D 旋转应走 2D 路径");
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_z = 45.0; // 仅 rotationZ 不触发 3D 矩阵
        assert!(build_gl_3d_matrix(&gl, 200.0, 160.0).is_none());
    }

    #[test]
    fn test_gl_3d_matrix_perspective() {
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        let m = build_gl_3d_matrix(&gl, 200.0, 160.0).expect("rotationX 应返回矩阵");
        let mut row = [0.0f32; 16];
        m.get_row_major(&mut row);
        // 有效相机距离 = max(8, max(200,160)) = 200
        let s = 45f32.to_radians().sin();
        let c = 45f32.to_radians().cos();
        let persp_y = row[3 * 4 + 1];
        let persp_z = row[3 * 4 + 2];
        assert!((persp_y + s / 200.0).abs() < 1e-6, "w 行 y 系数应为 -sinθ/cam：{persp_y}");
        assert!((persp_z + c / 200.0).abs() < 1e-6, "w 行 z 系数应为 -cosθ/cam：{persp_z}");
        // 相机越远透视越平
        let mut near = GraphicsLayerParams::default();
        near.rotation_x = 45.0;
        near.camera_distance = 2.0; // 低于视图尺寸 → 钳制到 200
        let mn = build_gl_3d_matrix(&near, 200.0, 160.0).unwrap();
        let mut rown = [0.0f32; 16];
        mn.get_row_major(&mut rown);
        let mut far = GraphicsLayerParams::default();
        far.rotation_x = 45.0;
        far.camera_distance = 500.0;
        let mf = build_gl_3d_matrix(&far, 200.0, 160.0).unwrap();
        let mut rowf = [0.0f32; 16];
        mf.get_row_major(&mut rowf);
        assert!(rowf[13].abs() < rown[13].abs(), "相机距离大 → 透视项小");
    }

    #[test]
    fn test_low_elevation_shadow_is_visible() {
        // Elevated rest=1 阴影必须可感知（“平时有高度”），hover=3 明显更高——
        // 矩形四周采样应比纯白背景明显更暗，防强度曲线被调回不可见。
        use skia_safe::{Color, Paint, surfaces};
        let measure = |elevation: f32| -> (i32, i32, i32) {
            let mut surface = surfaces::raster_n32_premul((120, 120)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::WHITE);
            let rect = skia_safe::Rect::from_xywh(20.0, 20.0, 80.0, 80.0);
            // 真实管线顺序：先垫底阴影，再画内容
            draw_elevation_shadow(
                canvas,
                rect,
                &crate::graphics::Shape::Rectangle,
                elevation,
                crate::graphics::Color { r: 0, g: 0, b: 0, a: 0x19 },
                crate::graphics::Color { r: 0, g: 0, b: 0, a: 0x40 },
            );
            let mut paint = Paint::default();
            paint.set_color(Color::WHITE);
            canvas.draw_rect(rect, &paint);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            let at = |x: usize, y: usize| -> [u8; 4] { px[y * 120 + x] };
            let bg = at(4, 4);
            let dark = |p: [u8; 4]| -> i32 {
                (bg[0] as i32 - p[0] as i32)
                    + (bg[1] as i32 - p[1] as i32)
                    + (bg[2] as i32 - p[2] as i32)
            };
            (
                dark(at(60, 101)), // 下方 1px
                dark(at(60, 19)),  // 上方 1px
                dark(at(19, 60)),  // 左侧 1px
            )
        };
        let (rest_below, rest_above, rest_left) = measure(1.0);
        let (hover_below, hover_above, hover_left) = measure(3.0);
        eprintln!(
            "probe elevation shadow: rest(1dp) below={rest_below} above={rest_above} left={rest_left} | \
             hover(3dp) below={hover_below} above={hover_above} left={hover_left}"
        );
        // The light sits directly above the component (`draw_elevation_shadow`), so a shadow is thrown DOWN and
        // to the sides: at 1 dp `above` measures 0 and only spills upward once the elevation is big enough to
        // wrap around (3 dp measures 48). Before the light was moved, a 1 dp shadow was thrown up and left.
        assert!(rest_left > 0, "rest 阴影应可见（左 1px）: {rest_left}");
        assert!(rest_below > 0, "rest 阴影应可见（下 1px）: {rest_below}");
        assert!(rest_below > rest_left, "rest 阴影应主要朝下: below={rest_below} left={rest_left}");
        assert!(hover_below > rest_below, "hover 下方阴影应高于 rest: rest={rest_below} hover={hover_below}");
        assert!(hover_above >= rest_above, "hover 上方阴影不应弱于 rest: rest={rest_above} hover={hover_above}");
        assert!(hover_left >= rest_left, "hover 左侧阴影不应弱于 rest: rest={rest_left} hover={hover_left}");
        assert!(
            hover_below > 0 && hover_above > 0 && hover_left > 0,
            "hover 阴影应在各方向可见: below={hover_below} above={hover_above} left={hover_left}"
        );
    }

    #[test]
    fn elevation_is_compressed_before_it_reaches_skia() {
        use super::paint::{compressed_elevation, ELEVATION_KNEE};

        assert_eq!(compressed_elevation(0.0), 0.0);
        assert_eq!(compressed_elevation(-2.0), 0.0);
        assert_eq!(compressed_elevation(f32::NAN), 0.0);

        let mut previous = 0.0;
        for elevation in [1.0, 2.0, 3.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 100.0] {
            let z = compressed_elevation(elevation);
            assert!(z > previous, "the curve is monotonic: {elevation} dp -> {z}");
            assert!(z <= elevation, "compression never grows a step: {elevation} -> {z}");
            assert!(z <= ELEVATION_KNEE + 1.0, "and bounded by the asymptote: {elevation} -> {z}");
            previous = z;
        }

        // Fixed anchors so a change of curve cannot pass silently.
        assert!(
            (compressed_elevation(1.0) - 1.0).abs() < 1e-4,
            "1 dp keeps its physical size: {}",
            compressed_elevation(1.0)
        );
        assert!(
            (compressed_elevation(ELEVATION_KNEE) - (ELEVATION_KNEE + 1.0) / 2.0).abs() < 1e-4,
            "the knee is the half-way point: {}",
            compressed_elevation(ELEVATION_KNEE)
        );
    }

    fn render_rect_with_gl(gl: &GraphicsLayerParams) -> (usize, usize, bool) {
        use skia_safe::{Color, Paint, surfaces};
        let mut surface = surfaces::raster_n32_premul((400, 200)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::BLACK);
        // 节点位于非原点 (40,30)、尺寸 200x160、transformOrigin 中心——
        // 复刻 render.rs 的 graphicsLayer 序列（枢轴必须含节点绝对位置）
        let (x0, y0, w, h) = (40.0f32, 30.0f32, 200.0f32, 160.0f32);
        let (px, py) = (w * 0.5, h * 0.5);
        let (pivot_x, pivot_y) = (x0 + px, y0 + py);
        let sf = 1.0f32;
        // 根平移：把节点中心（绝对 140,110）送到画布中心 (200,100)
        canvas.translate((200.0 - pivot_x * sf, 100.0 - pivot_y * sf));
        canvas.scale((sf, sf));
        apply_gl_transform(canvas, gl, x0, y0, w, h);
        let mut paint = Paint::default();
        paint.set_color(Color::WHITE);
        canvas.draw_rect(skia_safe::Rect::from_xywh(x0, y0, w, h), &paint);
        let pm = surface.peek_pixels().expect("pixmap");
        let px2: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: usize, y: usize| -> [u8; 4] { px2[y * 400 + x] };
        let center_white = at(200, 100) == [255, 255, 255, 255];
        let mut top = 200usize;
        let mut bottom = 0usize;
        for y in 0..200 {
            for x in (0..400).step_by(2) {
                if at(x, y) == [255, 255, 255, 255] {
                    if y < top { top = y; }
                    if y > bottom { bottom = y; }
                    break;
                }
            }
        }
        (top, bottom, center_white)
    }

    #[test]
    fn test_rotation_pivot_is_node_center() {
        // 非原点节点：旋转后中心像素必须仍在节点中心（枢轴含绝对位置）
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        gl.camera_distance = 150.0;
        let (top, bottom, center_white) = render_rect_with_gl(&gl);
        assert!(bottom > top, "矩形应可见：top={top} bottom={bottom}");
        assert!(center_white, "旋转应绕节点中心——中心像素应保持不动");
    }

    #[test]
    fn test_perspective_renders_in_full_sequence() {
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        gl.camera_distance = 300.0;
        let (top, bottom, _) = render_rect_with_gl(&gl);
        assert!(bottom > top, "矩形应可见：top={top} bottom={bottom}");
        // 与近似无透视（相机极远 → w≈1）的投影高度对比——透视会压缩投影
        let mut affine = GraphicsLayerParams::default();
        affine.rotation_x = 45.0;
        affine.camera_distance = 1.0e9;
        let (top_a, bottom_a, _) = render_rect_with_gl(&affine);
        let persp_h = (bottom - top) as f32;
        let affine_h = (bottom_a - top_a) as f32;
        assert!(
            (persp_h - affine_h).abs() > 2.0,
            "透视应显著压缩投影高度：persp_h={persp_h} affine_h={affine_h}"
        );
    }

    #[test]
    fn test_camera_distance_changes_projection() {
        let mut near = GraphicsLayerParams::default();
        near.rotation_x = 45.0;
        near.camera_distance = 20.0;
        let mut far = GraphicsLayerParams::default();
        far.rotation_x = 45.0;
        far.camera_distance = 500.0;
        let (t1, b1, _) = render_rect_with_gl(&near);
        let (t2, b2, _) = render_rect_with_gl(&far);
        assert_ne!((t1, b1), (t2, b2), "相机距离应改变投影结果");
    }

    #[test]
    fn test_rotation_never_disappears_with_default_camera() {
        // 默认相机 8 对 160dp 卡片：有效距离钳制到半尺寸，任意角度 w>0
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 75.0; // 75° 仍可见（90° 侧立成线是正确行为）
        let (top, bottom, _) = render_rect_with_gl(&gl);
        assert!(bottom > top, "rotationX=75 默认相机应仍可见：top={top} bottom={bottom}");
    }

    #[test]
    fn graphics_layer_color_filter_tints_content() {
        // 完整渲染路径回归：graphics_layer color_filter(Tint SrcIn) 必须
        // 经 saveLayer paint 染色层内内容（白背景 → 红）——render_pass1
        // 的 save_layer + paint color_filter 分支。
        use crate::runtime::composer::Composer;
        use crate::layout::constraints::Constraints;
        use crate::modifier::{Modifier};
        use crate::graphics::{BlendMode, ColorFilter, Shape};
        use skia_safe::{surfaces, Color as SkColor};

        let mut c = Composer::new();
        c.compose(|ctx| {
            let k = ctx.next_key();
            ctx.start_leaf(
                k,
                Modifier::new()
                    .size(100.0, 100.0)
                    .background(crate::graphics::Color::from_argb(255, 255, 255, 255), Shape::Rectangle)
                    .graphics_layer(crate::graphics::GraphicsLayerParams {
                        color_filter: Some(ColorFilter::Tint {
                            color: crate::graphics::Color::from_argb(255, 255, 0, 0),
                            blend_mode: BlendMode::SrcIn,
                        }),
                        ..crate::graphics::GraphicsLayerParams::default()
                    }),
            );
            ctx.end_node();
        });
        c.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        let root = c.layout_root_idx().unwrap();

        let mut surface = surfaces::raster_n32_premul((100, 100)).unwrap();
        surface.canvas().clear(SkColor::BLACK);
        crate::render::render(c.arena_nodes(), root, surface.canvas());

        let mut px = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        surface.read_pixels(&info, &mut px, 4, (50, 50));
        // 中心像素：白底被 Tint(SrcIn) 染成红（R 高、G/B 低）
        assert!(px[0] > 200 && px[1] < 60 && px[2] < 60,
            "中心像素应为红（染白→红），实际 {:?}", px);
    }

    #[test]
    fn test_clip_applies_before_transform() {
        use skia_safe::{Color, Paint, surfaces};
        let render = |clip: bool| -> usize {
            let mut surface = surfaces::raster_n32_premul((400, 200)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::BLACK);
            let (x0, y0, w, h) = (40.0f32, 30.0f32, 200.0f32, 160.0f32);
            let mut gl = GraphicsLayerParams::default();
            gl.rotation_z = 45.0;
            gl.clip = clip;
            canvas.save();
            if clip {
                canvas.clip_rect(Rect::new(x0, y0, x0 + w, y0 + h), None, true);
            }
            apply_gl_transform(canvas, &gl, x0, y0, w, h);
            let mut paint = Paint::default();
            paint.set_color(Color::WHITE);
            canvas.draw_rect(Rect::from_xywh(x0, y0, w, h), &paint);
            canvas.restore();
            let pm = surface.peek_pixels().unwrap();
            let px2: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
            px2.iter().filter(|p| **p == [255, 255, 255, 255]).count()
        };
        let unclipped = render(false);
        let clipped = render(true);
        assert!(clipped > 0, "clip 后内容应仍可见");
        assert!(
            unclipped > clipped,
            "clip 应在变换前裁掉旋转后的角落：unclipped={unclipped} clipped={clipped}"
        );
    }
