use super::*;

pub(crate) fn render_overlays(overlays: &[OverlayWindow], canvas: &skia_safe::Canvas, scale: f32, window: (f32, f32)) {
    for ov in overlays {
        // 显示进度 → (scale, alpha, dy, reveal)：progress State 驱动（peek——
        // 渲染期零重组）。打开：progress 0→1，apply() 正向；关闭（closing）：
        // progress 1→0，apply_exit() 反向。dy = 高度倍数位移；reveal = 揭示
        // 高度倍数（顶部展开用）。
        let (anim_scale, anim_alpha, anim_dy, anim_reveal) = match (&ov.progress, ov.closing, &ov.enter_anim, &ov.exit_anim) {
            (Some(p), false, Some(spec), _) => spec.apply(p.peek()),       // 进入
            (Some(p), true, _, Some(spec)) => spec.apply_exit(p.peek()),   // 退出
            _ => (1.0, 1.0, 0.0, 1.0),                                     // 无动画
        };
        // 模态遮罩（淡入淡出——跟随内容 alpha）
        if ov.modal {
            let mut mask = skia_safe::Paint::default();
            mask.set_color(skia_safe::Color::from_argb((110.0 * anim_alpha) as u8, 0, 0, 0));
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(0.0, 0.0, window.0 * scale, window.1 * scale),
                &mask,
            );
        }
        let Some(r) = ov.composer.layout_root_idx() else { continue; };
        let nodes = ov.composer.arena_nodes();
        canvas.save();
        canvas.translate((ov.screen_pos.0 * scale, ov.screen_pos.1 * scale));
        // 进入/退出动画：围绕 overlay 中心缩放 + 内容淡入淡出 + 垂直滑入。
        // ⚠ scale 在 translate 之后——先定位再缩放（缩放中心 = overlay 左上角 +
        // 内容半尺寸，即内容中心）；slide 在缩放之后、内容绘制之前（布局空间位移）
        let size = ov.composer.layout_root()
            .map(|r| (r.measured_size.width, r.measured_size.height))
            .unwrap_or((0.0, 0.0));
        // Slide/揭示期间裁到内容框（对齐上游下拉 `.clip(dropdownShape)` 与
        // 展开揭示）——否则滑入起点整个面板压住 bar。clip 在 settled bounds 上；
        // 动画结束内容恰好落回框内 → 无跳变。reveal<1 时按揭示高度裁。
        // (anim_dy != 0.0) 或 (anim_reveal < 1.0) 时裁剪。
        // The clip height comes from `overlay_reveal_clip_height`, which also keeps "no clip" (`None`)
        // distinct from "clipped to zero" — the distinction the render trace needs.
        let clip_h = crate::overlay::overlay_reveal_clip_height(size.1, anim_dy, anim_reveal)
            .unwrap_or(-1.0);
        // Render-time trace: the overlay's reveal/alpha/offset exist only here, at paint time — the debug
        // server's tree reports POST-LAYOUT sizes, so a reveal (a clip) was invisible to every other
        // channel (`reveal` and `dy` are fractions; `clip_h` is what is actually applied).
        if crate::debug::animation::enabled() {
            let mut r = crate::debug::animation::TraceRecord::render(ov.id);
            r.phase = Some(if ov.closing { "closing" } else { "opening" });
            r.alpha = Some(anim_alpha);
            r.clip = Some(clip_h >= 0.0);
            // `painted.h` is the height actually drawn: `clip_h` when revealing, and the FULL height when
            // the animation is settled (`clip_h == -1` means "no clip", not "zero height" — reporting 0
            // there made a fully open panel look empty).
            let painted_h = if clip_h >= 0.0 { clip_h } else { size.1 };
            r.painted = Some(crate::debug::animation::TraceRect {
                x: ov.screen_pos.0,
                y: ov.screen_pos.1,
                w: size.0,
                h: painted_h,
            });
            r.detail = Some(format!(
                "reveal={anim_reveal:.4} dy={anim_dy:.4} scale={anim_scale:.4} layout_h={:.1}",
                size.1
            ));
            crate::debug::animation::record(r);
        }
        if clip_h >= 0.0 {
            // clip_rect is affected by the current matrix (screen_pos translate
            // applied above, no content scale yet) — coordinates are device px,
            // so multiply layout units by scale (same as the scrim rect above).
            canvas.clip_rect(
                skia_safe::Rect::from_xywh(0.0, 0.0, size.0 * scale, clip_h * scale),
                None,
                Some(false),
            );
        }
        if anim_scale != 1.0 {
            // Scale around the pivot the animation asks for: material3's menus grow out of their anchor
            // (`anchor_pivot`, the origin `calculateTransformOrigin` picks), everything else keeps the
            // content centre it has always used.
            let (px_frac, py_frac) = match (ov.closing, &ov.enter_anim, &ov.exit_anim) {
                (false, Some(spec), _) if spec.anchor_pivot => ov
                    .anchor_rect
                    .map(|anchor| {
                        crate::overlay::overlay_transform_origin(
                            anchor,
                            (ov.screen_pos.0, ov.screen_pos.1, size.0, size.1),
                        )
                    })
                    .unwrap_or((0.5, 0.5)),
                (true, _, Some(spec)) if spec.anchor_pivot => ov
                    .anchor_rect
                    .map(|anchor| {
                        crate::overlay::overlay_transform_origin(
                            anchor,
                            (ov.screen_pos.0, ov.screen_pos.1, size.0, size.1),
                        )
                    })
                    .unwrap_or((0.5, 0.5)),
                _ => (0.5, 0.5),
            };
            canvas.translate(((px_frac * size.0) * scale, (py_frac * size.1) * scale));
            canvas.scale((anim_scale, anim_scale));
            canvas.translate((-(px_frac * size.0) * scale, -(py_frac * size.1) * scale));
        }
        if anim_dy != 0.0 {
            canvas.translate((0.0, anim_dy * size.1 * scale));
        }
        if anim_alpha < 1.0 {
            // 内容淡入/淡出：整体 alpha 层（save_layer_alpha_f——Skia 层叠 alpha）
            canvas.save_layer_alpha_f(None, anim_alpha);
        }
        // overlay 内容与主树一致按 scale 绘制（坐标均为逻辑单位）——
        // 缺省会导致内容以 1x 绘制：可见位置/大小与命中测试（逻辑坐标）错位
        canvas.scale((scale, scale));
        crate::render::render(nodes, r, canvas);
        // Transition layer (sources + elevated endpoints), overlay-local coords.
        ov.composer.render_layer(canvas);
        if anim_alpha < 1.0 {
            canvas.restore();
        }
        canvas.restore();
    }
}
