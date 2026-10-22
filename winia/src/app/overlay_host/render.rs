use super::*;

pub(crate) fn render_overlays(overlays: &[OverlayWindow], canvas: &skia_safe::Canvas, scale: f32, window: (f32, f32)) {
    for ov in overlays {
        // Show progress → (scale, alpha, dy, reveal): driven by the progress State (peek —
        // zero recomposition during render). Opening: progress 0→1, apply() forward; closing:
        // progress 1→0, apply_exit() reverse. dy = height-multiple offset; reveal = revealed
        // height multiple (used for top expansion).
        let (anim_scale, anim_alpha, anim_dy, anim_reveal) = match (&ov.progress, ov.closing, &ov.enter_anim, &ov.exit_anim) {
            (Some(p), false, Some(spec), _) => spec.apply(p.peek()),       // entering
            (Some(p), true, _, Some(spec)) => spec.apply_exit(p.peek()),   // exiting
            _ => (1.0, 1.0, 0.0, 1.0),                                     // no animation
        };
        // Modal scrim (fades in/out — follows the content alpha).
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
        // Enter/exit animation: scale around the overlay centre + content fade + vertical slide-in.
        // ⚠ scale comes after translate — position first, then scale (the scale centre = the overlay's top-left
        // + half the content size, i.e. the content centre); slide comes after the scale and before content
        // drawing (a layout-space displacement).
        let size = ov.composer.layout_root()
            .map(|r| (r.measured_size.width, r.measured_size.height))
            .unwrap_or((0.0, 0.0));
        // While sliding/revealing, clip to the content box (matching upstream's `.clip(dropdownShape)` on the dropdown
        // and the expansion reveal) — otherwise the whole panel covers the bar at the slide start. The clip is on the
        // settled bounds; once the animation ends the content sits exactly inside the box → no jump. When reveal<1, clip to the revealed height.
        // Clip when (anim_dy != 0.0) or (anim_reveal < 1.0).
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
            // Content fade in/out: an overall alpha layer (save_layer_alpha_f — Skia layers the alpha).
            canvas.save_layer_alpha_f(None, anim_alpha);
        }
        // Overlay content is drawn with the same scale as the main tree (all coordinates are logical units) —
        // without it the content is drawn at 1x and its visible position/size no longer matches hit-testing (logical coordinates).
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
