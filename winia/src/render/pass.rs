use super::*;
use super::backdrop::*;
use super::helpers::*;
use super::paint::*;
use super::text_field::*;

// ── 渲染遍历（含 BackdropBlur 即时处理）──

thread_local! {
    static CAPTURING_ITEM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Record one item subtree into its exit picture before the normal draw. The recording call is
/// guarded so the recursive render cannot capture itself again.
fn capture_item(
    nodes: &[LayoutNode],
    root_idx: usize,
    idx: usize,
    x: f32,
    y: f32,
    paint: &crate::layout::item_animation::ItemPaint,
) {
    if paint.fade_out.is_none() || CAPTURING_ITEM.with(|c| c.get()) {
        return;
    }
    let node = &nodes[idx];
    let content = node.content_box();
    if content.width <= 0.0 || content.height <= 0.0 {
        return;
    }
    let visual = paint.offset.peek();
    let captured_position = crate::unit::Offset::new(x + visual.x, y + visual.y);
    {
        let store = paint.exit_store.lock().unwrap_or_else(|e| e.into_inner());
        if store.pictures.get(&paint.key).is_some_and(|old| {
            old.position == captured_position
                && old.size == crate::unit::Size::new(content.width, content.height)
                && visual == crate::unit::Offset::ZERO
                && paint.alpha.peek() >= 0.999
        }) {
            return;
        }
    }
    CAPTURING_ITEM.with(|c| c.set(true));
    let mut recorder = skia_safe::PictureRecorder::new();
    let bounds = Rect::new(0.0, 0.0, content.width, content.height);
    {
        let canvas = recorder.begin_recording(bounds, false);
        canvas.translate((-x, -y));
        render_pass1(
            nodes,
            root_idx,
            idx,
            canvas,
            x - node.position.x,
            y - node.position.y,
            true,
            true,
        );
    }
    let picture = recorder.finish_recording_as_picture(None);
    CAPTURING_ITEM.with(|c| c.set(false));
    if let Some(picture) = picture {
        paint.exit_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pictures
            .insert(
                paint.key,
                crate::layout::item_animation::CapturedItem {
                    picture,
                    position: captured_position,
                    size: crate::unit::Size::new(content.width, content.height),
                },
            );
    }
}

/// Draw the retained exit pictures after the live tree. Each store owns its own exits, so two
/// lists cannot steal each other's ghosts.
fn draw_host_exits(canvas: &Canvas, host: &crate::layout::item_animation::ItemAnimationHost) {
    let mut store = host.store().lock().unwrap_or_else(|e| e.into_inner());
    let mut finished = Vec::new();
    for (key, exit) in store.exits.iter() {
        let alpha = exit.alpha.peek().clamp(0.0, 1.0);
        if alpha > 0.001 {
            canvas.save();
            canvas.translate((exit.position.x, exit.position.y));
            let mut paint = Paint::default();
            paint.set_alpha_f(alpha);
            canvas.draw_picture(&exit.picture, None, Some(&paint));
            canvas.restore();
        }
        if alpha <= 0.001
            && !crate::animation::has_animation_for_state(exit.alpha.state_id())
        {
            finished.push(*key);
        }
    }
    for key in finished {
        store.exits.remove(&key);
    }
}

/// The item translate/opacity scope, restored by Drop so a flight early-return cannot leak canvas
/// state into the rest of the frame.
struct ItemCanvasGuard<'a> {
    canvas: &'a Canvas,
    active: bool,
    layer: bool,
}

impl<'a> ItemCanvasGuard<'a> {
    fn new(
        canvas: &'a Canvas,
        paint: Option<&crate::layout::item_animation::ItemPaint>,
        bounds: Rect,
    ) -> Self {
        let Some(paint) = paint else {
            return Self { canvas, active: false, layer: false };
        };
        canvas.save();
        let offset = paint.offset.peek();
        canvas.translate((offset.x, offset.y));
        let alpha = paint.alpha.peek().clamp(0.0, 1.0);
        let layer = alpha < 0.999;
        if layer {
            canvas.save_layer_alpha_f(Some(bounds), alpha);
        }
        Self { canvas, active: true, layer }
    }
}

impl Drop for ItemCanvasGuard<'_> {
    fn drop(&mut self) {
        if self.layer {
            self.canvas.restore();
        }
        if self.active {
            self.canvas.restore();
        }
    }
}

pub(super) fn render_pass1(
    nodes: &[LayoutNode],
    root_idx: usize,
    idx: usize,
    canvas: &Canvas,
    parent_x: f32, parent_y: f32,
    layer_root: bool,
    // This subtree is painted on a ROOTLESS canvas (a layer entry above it):
    // the ancestor translate is not on the canvas, so a transitioning node
    // inside must not add its frozen scroll sum. `layer_root` cannot answer
    // this — it is about the node itself, this is about the canvas.
    rootless: bool,
) {
    let node = &nodes[idx];
    // Anything that is not painted in place is skipped here: a flight end lifted into the transition
    // layer (a canvas clip can never be un-set by a descendant, so the layer paints it after the whole
    // tree — that is what lets it land above non-shared siblings), chrome that opted into the scope
    // overlay, and a PLACEHOLDER (a marked copy of a key a flight already owns: the flight paints the
    // animated rect, so a second static copy must not appear). Layout, state and hit testing are
    // untouched; only paint moves.
    if !layer_root && node.paint_disposition() != PaintDisposition::InTree {
        return;
    }
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    // Content box, NOT `measured_size`: while a flight reports a placeholder
    // size (Compose `PlaceHolderSize`) the latter is what the PARENT was told,
    // so painting, clipping and child placement must use this instead.
    let content_box = node.content_box();
    let w = content_box.width;
    let h = content_box.height;
    if w <= 0.0 || h <= 0.0 { return; }

    // Item animation (Compose's `animateItem`): the item is painted where it WAS, and the offset
    // travels to 0 as the layout settles; the alpha layer covers the whole item for enter/exit fade.
    let item_host = node.modifier.item_animation_host().cloned();
    let item_paint = if CAPTURING_ITEM.with(|c| c.get()) {
        None
    } else {
        node.item_paint.clone()
    };
    if let Some(paint) = &item_paint {
        capture_item(nodes, root_idx, idx, x, y, paint);
    }
    let _item_canvas = ItemCanvasGuard::new(
        canvas,
        item_paint.as_ref(),
        Rect::new(x, y, x + w, y + h),
    );

    let rect = Rect::new(x, y, x + w, y + h);

    // The box this node's OWN content occupies: `rect` minus whatever its
    // `Modifier::minimum_interactive_size` moved inward. Compose gets the same split by nesting —
    // `InteractiveComponentSize.kt:125-133` measures an inner `.size(40)` and reports `max(content, 48)`
    // for the outer box — while winia has one flat box.
    //
    // Used by the background, border, ripple, backdrop blur, the graphics layer (its clip AND its
    // transform) and the focus ring. NOT by everything: `draw_shadow_layer`, the elevation-shadow shape,
    // custom draw nodes, draw nodes and wrapping draw nodes still take `rect` — none of which any in-repo
    // component pairs with a minimum-interactive inset.
    //
    // It comes from the node's inset FIELD, not from the `MinimumInteractive*AlignmentLine`s: a container
    // merges its children's alignment lines (`layout/node.rs:3346-3367`), so reading the LINE would inset
    // the background of every ancestor that merely CONTAINS a Button — the expanded SearchBar panel among
    // them. Padding is deliberately NOT folded in: it has its own meaning for text and children, and
    // composing the two would move the background of every padded component (Badge, SearchBar and
    // SplitButton rely on their background filling the node).
    // A node whose own minimum-interactive modifier grew its box paints its CONTENT box, centred in
    // the box it ended up with — which is what Compose does (the modifier centres the content and
    // the content keeps its own size). Sizing the paint from the touch target instead took the
    // difference off BOTH sides, so a parent that forced the node narrower than the target (a
    // ButtonGroup squeezing an item) got a box narrower than the layout asked for, offset from the
    // slot it was placed in.
    let paint_rect = if let Some((left, top, pw, ph)) = node.minimum_interactive_painted {
        // A parent forced the node and said exactly what to paint (a squeezed ButtonGroup item).
        Rect::new(x + left, y + top, x + left + pw, y + top + ph)
    } else {
        match node.minimum_interactive_content {
            Some((content_w, content_h)) => {
                // CENTRED — that is what the modifier does with its content. A parent that wants it
                // somewhere else says so through `minimum_interactive_painted` above.
                let (left, top) = ((w - content_w) / 2.0, (h - content_h) / 2.0);
                Rect::new(x + left, y + top, x + left + content_w, y + top + content_h)
            }
            None => Rect::new(x, y, x + w, y + h),
        }
    };

    // Shared-element flight visuals (Tier 0): outermost morph — pure
    // render-phase (translate/scale/alpha/clip), zero recomposition.
    // Backdrop blur snapshots AFTER the transform (whatever lies beneath the
    // visual position); backdrop-blur heroes are a documented limitation.
    // Nested limitation: a transitioning descendant of a transitioning node
    // compounds both transforms (no ancestor-flight guard) — do not put
    // shared markers on descendants of shared markers.
    // bg/border/clip-element morphs consume `tf_radii` below (radii pairs in
    // layout space — pre-divided by the flight axis scales).
    let (tf_saved, tf_layered, tf_radii, tf_scale): (
        bool,
        bool,
        Option<[(f32, f32); 4]>,
        // The scale the flight transform applies; `RoundedCorner` needs it to turn its
        // device-space radius into the pre-divided one the canvas expects.
        Option<(f32, f32)>,
    ) =
        if let Some(t) = node.transition.as_ref() {            // Unclamped progress: spring overshoot (t > 1 / t < 0) flies past
            // the endpoint — the Compose spring look. Every consumer
            // (lerp, radii, clip, hit) shares this single t; only opacity
            // stays clamped (alpha()) and degenerate rects stay invisible.
            // Paint follows the SAME arc-aware lerped() as hit (a raw
            // start.lerp here would silently straighten arc flights).
            let l = t.lerped();
            let alpha = t.alpha();
            if alpha <= 0.001 || l.width <= 0.0 || l.height <= 0.0 {
                return;
            }
            // Render-time trace (node level): the flight's painted rect, radii, clip and alpha are all
            // produced HERE and stored nowhere — the layout tree only knows the end's own layout box, and the
            // flight records describe the flight's decision rather than the geometry this call was handed.
            // Recorded before the early-return above would have skipped it, i.e. only what is really painted.
            if crate::debug::animation::enabled() {
                let mut r = crate::debug::animation::TraceRecord::render(node.slot_key);
                r.role = Some(match t.role {
                    TransitionRole::Source => "Source",
                    TransitionRole::Target => "Target",
                    TransitionRole::Morph => "Morph",
                });
                r.flight = Some(t.flight);
                r.progress = Some(t.progress);
                r.painted = Some(crate::debug::animation::TraceRect { x: l.x, y: l.y, w: l.width, h: l.height });
                r.alpha = Some(alpha);
                r.radii = Some(t.radii());
                r.clip = Some(t.clip);
                r.detail = Some(format!("node={:#x}", node.id));
                crate::debug::animation::record(r);
            }
            canvas.save();
            // Expand wipes in from the edge — force the lerped-bounds clip
            // even when the marker asked for none (else content overflows).
            // Morphs never run channels (same content, no appear/disappear).
            let fx_clip = match (&t.role, t.bounds_fx.as_ref()) {
                (TransitionRole::Morph, _) | (_, None) => false,
                (TransitionRole::Target, Some((enter, _))) => enter.expand || enter.expand_h,
                (TransitionRole::Source, Some((_, exit))) => exit.expand || exit.expand_h,
            };
            if t.clip || fx_clip {
                canvas.clip_rrect(t.canvas_rrect(), None, Some(true));
            }
            // Ancestor scroll add-back (BLOCKER #1): the canvas carries
            // translate(-S) from scrolled ancestors while `l` is
            // scroll-corrected — painting at `l + S` lands exactly on the
            // lerped rect (the frame hit test and ghost routing use).
            // Pivot stays the pure layout origin: content is drawn at layout
            // coords, so final(p) = l + S - S + s*(p - node_origin).
            // A LAYER ROOT renders on a canvas with no ancestor translate and
            // at a scroll-corrected origin, so it must not add the sum back —
            // the same bit that routed this node to the layer (skip in the
            // walk, paint here) decides the add-back too, so the two can never
            // disagree.
            // The canvas decides, not the node: a layer entry is painted with no
            // ancestor translate, so neither it nor anything below it may add
            // the frozen scroll sum back (that would double-count).
            let scroll = if rootless { (0.0, 0.0) } else { t.scroll };
            canvas.translate((l.x + scroll.0, l.y + scroll.1));
            // sharedBounds enter/exit channels, evaluated at appearance
            // amount q (target appears with p, source disappears with 1-p).
            // Slide offsets, scale k and expand (≈ scale-about-edge; layout
            // followers already sit at end state under JumpCut, so expand
            // reads as scale+clip). Element flights and morphs skip.
            // ORDER: channels live in DEVICE space (right after the flight
            // translate, before the flight scale) — appending them after
            // scale would multiply offsets by the axis scales (which differ
            // per end: the ghost grows while the target shrinks).
            if let Some((enter, exit)) = t.bounds_fx.as_ref() {
                let (cfg, q) = match t.role {
                    TransitionRole::Target => (enter, t.progress),
                    TransitionRole::Source => (exit, 1.0 - t.progress),
                    TransitionRole::Morph => (enter, 1.0),
                };
                if !matches!(t.role, TransitionRole::Morph) {
                    if let Some((dir, off)) = cfg.slide {
                        let full = match dir {
                            SlideDirection::Left | SlideDirection::Right => l.width,
                            SlideDirection::Up | SlideDirection::Down => l.height,
                        };
                        let dist = match off {
                            SlideOffset::Fixed(px) => px,
                            SlideOffset::Fraction(f) => f * full,
                        };
                        let o = (1.0 - q) * dist;
                        let (ox, oy) = match dir {
                            SlideDirection::Left => (-o, 0.0),
                            SlideDirection::Right => (o, 0.0),
                            SlideDirection::Up => (0.0, -o),
                            SlideDirection::Down => (0.0, o),
                        };
                        canvas.translate((ox, oy));
                    }
                    // Scale pivot is RELATIVE to the lerped origin (this block
                    // sits before the flight scale — absolute device pivots
                    // would be scaled along; same trap as the slide above).
                    // NOTE: expand takes over the scale pivot (single merged
                    // k) — combining scale+expand on one endpoint is
                    // discouraged (Compose layers them; v1 merges).
                    let mut kx = 1.0f32;
                    let mut ky = 1.0f32;
                    let mut px = l.width * cfg.transform_origin.0;
                    let mut py = l.height * cfg.transform_origin.1;
                    if cfg.scale {
                        let k = cfg.scale_from + (1.0 - cfg.scale_from) * q;
                        kx *= k;
                        ky *= k;
                    }
                    if cfg.expand {
                        ky *= q;
                        px = l.width * 0.5;
                        py = match cfg.expand_from {
                            ExpandFrom::Top => 0.0,
                            ExpandFrom::Bottom => l.height,
                        };
                    }
                    if cfg.expand_h {
                        kx *= q;
                        px = match cfg.expand_from_h {
                            ExpandFromH::Start => 0.0,
                            ExpandFromH::End => l.width,
                        };
                        py = l.height * 0.5;
                    }
                    if (kx - 1.0).abs() > 1e-6 || (ky - 1.0).abs() > 1e-6 {
                        canvas.translate((px, py));
                        canvas.scale((kx, ky));
                        canvas.translate((-px, -py));
                    }
                }
            }
            // Scale the content box into the lerped rect. `paint_scale` owns the
            // rule (and is unit-tested): a RE-MEASURED end must not scale at all
            // — its content was already laid out at the animated size, and the
            // box trails `lerped()` by one poll (writers run after layout), so
            // scaling by `l/box` would stretch the freshly re-flowed content by
            // the frame delta (measured 1.22-1.29x in the real app loop).
            // Everything else scales its content box into the lerped rect.
            let (sx, sy) = t.paint_scale(w, h);
            canvas.scale((sx, sy));
            // Scale about the lerped origin (not the canvas origin): content
            // drawn at layout coords must land on the lerped rect, i.e.
            // final(p) = l + s*(p - node_origin). Same pivot convention as
            // apply_gl_transform and remap_hit. The offset places the content by
            // Compose `Alignment` when the fit does not fill both axes (it is already
            // in node coords, i.e. divided by the scale, because it is applied inside
            // `canvas.scale`).
            let (off_x, off_y) = t.paint_offset(w, h);
            canvas.translate((-x + off_x, -y + off_y));
            let mut layered = false;
            if alpha < 0.999 {
                let mut paint = skia_safe::Paint::default();
                paint.set_alpha_f(alpha);
                canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&paint));
                layered = true;
            }
            (true, layered, Some(t.radii_pairs(
                if t.remeasure { l.width } else { w },
                if t.remeasure { l.height } else { h },
            )), Some((sx, sy)))
        } else {
            (false, false, None, None)
        };
    // Layout-space morph rect for the Clip-element arm (lands exactly on the
    // lerped bounds under the flight transform, by linearity) — shaped by Compose's
    // `overlayClip` read from THIS node's marker (per-end, like Compose): the flight's own
    // corner quad by default, square corners for `Rectangle`, a device-space radius for
    // `RoundedCorner` (resolved on the lerped rect), and no clip at all for `None`.
    let tf_clip_rr: Option<skia_safe::RRect> = match (tf_radii, tf_scale) {
        (Some(r), Some((sx, sy))) => {
            match crate::transition::overlay_clip_of(&node.modifier) {
                crate::transition::OverlayClip::None => None,
                crate::transition::OverlayClip::Rectangle => {
                    Some(skia_safe::RRect::new_rect(rect))
                }
                crate::transition::OverlayClip::RoundedCorner(radius) => {
                    // The radius is device-space, so pre-divide by the paint scale — the
                    // same convention `radii_pairs` uses (the clip is drawn inside
                    // `canvas.scale`).
                    let rx = radius / sx.abs().max(1e-6);
                    let ry = radius / sy.abs().max(1e-6);
                    Some(skia_safe::RRect::new_rect_xy(rect, rx, ry))
                }
                crate::transition::OverlayClip::Bounds => {
                    Some(skia_safe::RRect::new_rect_radii(
                        rect,
                        &crate::transition::rrect_vectors(r),
                    ))
                }
            }
        }
        _ => None,
    };

    // 背景模糊：在节点**自身任何内容（背景/文本/子节点）绘制之前**处理——
    // snapshot 只含"位于其下"的已画内容（祖先 + 前面的兄弟），对齐 Compose
    // backdropBlur 语义。参考 v1 item.rs 实现：物理像素 snapshot + CropRect
    // blur + clip 到节点 + 像素网格对齐画回（draw_image 无采样缩放）。
    if let Some(radius) = node.modifier.backdrop_blur_radius() {
        // The blurred region is the container, not the node: the node may have been grown to the 48 dp
        // touch target, and blurring that samples outside the control that owns it.
        draw_backdrop_blur(
            canvas,
            paint_rect.x(),
            paint_rect.y(),
            paint_rect.width(),
            paint_rect.height(),
            radius,
        );
    }

    // 图形层：包住整个节点（background + text + children），应用 alpha/变换/颜色滤镜
    let gl_params = node.modifier.graphics_layer_params();
    // Render-time trace (node level): the alpha a `graphics_layer` contributed exists only here — it is read
    // from the modifier chain at paint time and never stored. A `Modifier::alpha` or an animated layer alpha
    // (a fade) was therefore invisible to every other channel.
    if crate::debug::animation::enabled() {
        if let Some(gl) = gl_params.as_ref() {
            if gl.alpha < 1.0 || gl.scale_x != 1.0 || gl.scale_y != 1.0
                || gl.translation_x != 0.0 || gl.translation_y != 0.0
                || gl.rotation_z != 0.0
            {
                let mut r = crate::debug::animation::TraceRecord::render(node.slot_key);
                r.phase = Some("graphics_layer");
                r.alpha = Some(gl.alpha);
                r.layout = Some(crate::debug::animation::TraceRect { x, y, w, h });
                r.detail = Some(format!(
                    "scale=({:.4},{:.4}) translate=({:.2},{:.2}) rot_z={:.2} clip={}",
                    gl.scale_x, gl.scale_y, gl.translation_x, gl.translation_y, gl.rotation_z, gl.clip
                ));
                crate::debug::animation::record(r);
            }
        }
    }
    let gl_saved = if let Some(gl) = gl_params {
        let has_alpha = gl.alpha < 1.0;
        if graphics_layer_needs_layer(&gl) {
            let mut paint = skia_safe::Paint::default();
            if let Some(cf) = &gl.color_filter {
                if let Some(filter) = to_skia_color_filter(cf) {
                    paint.set_color_filter(filter);
                }
            }
            if has_alpha {
                paint.set_alpha_f(gl.alpha.clamp(0.0, 1.0));
            }
            if let Some(crate::graphics::RenderEffect::Blur { radius }) = &gl.render_effect {
                if *radius > 0.0 {
                    if let Some(filter) = skia_safe::image_filters::blur(
                        (*radius, *radius),
                        skia_safe::TileMode::Clamp,
                        None,
                        None,
                    ) {
                        paint.set_image_filter(filter);
                    }
                }
            }
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&paint));
        } else {
            canvas.save();
        }
        // graphicsLayer shadowElevation：独立“变换 → 绘制 → 恢复”——阴影
        // 随 3D 形变，且不参与内容 clip（对标 Compose：层阴影在边界外）
        if gl.shadow_elevation > 0.0 {
            canvas.save();
            // Transform around the container's centre, for the same reason.
            apply_gl_transform(
                canvas,
                &gl,
                paint_rect.x(),
                paint_rect.y(),
                paint_rect.width(),
                paint_rect.height(),
            );
            let shape = gl.shadow_shape.clone().unwrap_or(crate::graphics::Shape::Rectangle);
            draw_elevation_shadow(
                canvas,
                rect,
                &shape,
                gl.shadow_elevation,
                gl.ambient_shadow_color,
                gl.spot_shadow_color,
            );
            canvas.restore();
        }
        // Clip to the layer's bounds, BEFORE the transform (Compose semantics: clip to the layer bounds
        // first and transform second, so a rotation cuts the corners off — clipping after the transform
        // would rotate with the content and cancel itself out). `paint_rect`: the layer's bounds are the
        // container, not the 48 dp touch target the node was grown to.
        if gl.clip {
            canvas.clip_rect(paint_rect, None, true);
        }
        // `paint_rect` too: the layer's bounds are the container, not the 48 dp touch target the node was
        // grown to — the same box the shadow transform above rotates around.
        apply_gl_transform(
            canvas,
            &gl,
            paint_rect.x(),
            paint_rect.y(),
            paint_rect.width(),
            paint_rect.height(),
        );
        true
    } else { false };
    let mut blur_radius: Option<f32> = None;
    let mut clip_shape: Option<crate::graphics::Shape> = None;
    // 阴影（elevation, shape, color）——链序中与 background 同层绘制；
    // clip=true 时并入 clip_shape（内容裁剪，阴影不受裁——Compose 语义）
    // 阴影层（预扫描直接绘制——见下方 pre-scan 注释）
    let mut text: Option<(&str, f32, &crate::graphics::Color, usize, crate::text::TextAlign, crate::text::TextOverflow, crate::text::FontWeight, crate::text::FontSlant, bool, f32, Option<f32>)> = None;
    let mut scroll_offset_v: Option<f32> = None;
    let mut scroll_offset_h: Option<f32> = None;

    // 叶子 padding 渲染偏移（测量已回加尺寸——绘制内容按内边距内缩；
    // 解析与测量期 get_padding_sides 一致：静态/动态统一，RTL 时 start 在右）
    let (pad_s, pad_t, pad_e, pad_b) = node.modifier.get_padding_sides();
    let pad_rtl = node.layout_direction == crate::layout::LayoutDirection::Rtl;
    let (content_x, content_y, content_w, content_h) = match node.content_box_override {
        // `paddingFrom` measured the content's own box against its alignment line — how much padding
        // the line needs is only known after the content is measured, so the node carries the result
        // instead of the modifier being asked again. A container moves its children and leaves this
        // `None`, falling through to the padding arithmetic below.
        Some((inset_x, inset_y, w_inner, h_inner)) => {
            (x + inset_x, y + inset_y, w_inner, h_inner)
        }
        None => (
            x + if pad_rtl { pad_e } else { pad_s },
            y + pad_t,
            (w - pad_s - pad_e).max(0.0),
            (h - pad_t - pad_b).max(0.0),
        ),
    };

    // `paint_rect` (computed beside `rect` above) is the box the backgrounds and borders use.

    // ⚠ 阴影必须**垫底**（主循环绘制背景之前——无论链序）：Compose shadow
    // 是 graphicsLayer 独立层（垫底）。此前预扫描代码误放在主循环之后——
    // 阴影画在背景之上 → 卡片被压暗（绿卡 ×(1-0.43)≈0.6——实测 bug）
    for el in node.modifier.elements() {
        if let ModifierElement::Shadow { params, shape, .. } = el {
            let shape = shape.eval();
            let params = params.eval();
            draw_shadow_layer(canvas, rect, &shape, &params);
        }
    }

    // 同节点链序中最后绘制的背景（颜色+形状）——边框色与形状都等于它时
    // 跳过描边（合并为纯填充），避免半透明同色双重混合
    let mut last_background: Option<(crate::graphics::Color, crate::graphics::Shape)> = None;
    for el in node.modifier.elements() {
        match el {
            ModifierElement::Blur { radius } => {
                blur_radius = Some(radius.eval());
            }
            ModifierElement::Clip { shape } => {
                clip_shape = Some(shape.eval());
            }
            ModifierElement::Shadow { shape, clip, .. } => {
                if *clip {
                    clip_shape = Some(shape.eval());
                }
            }
            ModifierElement::VerticalScroll { state } => {
                let mut off = state.offset.get();
                // reverseLayout（LazyColumn 反向）：滚动平移镜像为
                // scroll_origin = content - vh - offset（内容坐标从底向上排布）
                if node.scroll_reverse {
                    off = (node.scroll_content_height - node.scroll_viewport_height - off).max(0.0);
                }
                scroll_offset_v = Some(off);
            }
            ModifierElement::HorizontalScroll { state, .. } => {
                let mut off = state.offset.get();
                if node.scroll_reverse {
                    off = (node.scroll_content_width - node.scroll_viewport_width - off).max(0.0);
                }
                scroll_offset_h = Some(off);
            }
            // 文本输入框容器（M3 Filled/Outlined——背景/指示线/边框/label/支持文本）
            ModifierElement::TextFieldVisual { variant, shape, colors, enabled: _, read_only: _, cursor_color: _, indicator_color, focus_progress, offset_mapping: _, supporting } => {
                // 容器 rect：有支持文本时扣除其区域（supporting 画在容器底部外
                // 4dp，节点总高 = 容器 + 4 + 16）
                let supporting_h = supporting.as_ref().map_or(0.0, |sv| sv.height());
                let container_rect = Rect::new(x, y, x + w, y + h - supporting_h);
                // ⚠ label/placeholder/图标/前后缀均为子节点（text-field-v2
                // 容器化——TextFieldLayout 定位）；Outlined label 缺口由子节点
                // Label 的 placement 构造（跨边框悬浮时——label 顶越出容器顶，
                // 缺口 = label 水平范围 ± 4dp（M3 populated label padding））
                let cutout = if *variant == crate::text::field::TextFieldVariant::Outlined {
                    node.children.iter().find_map(|&ci| {
                        let cn = &nodes[ci];
                        let is_label = cn.modifier.elements().iter().any(|el| {
                            matches!(el, ModifierElement::TextFieldSlot { role }
                                if *role == crate::text::field::TextFieldSlotRole::Label)
                        });
                        if !is_label { return None; }
                        // ⚠ 子节点 position 相对**容器**（本节点）——用本节点
                        // 绝对坐标 x/y（parent_x 是父层坐标——用错缺口画偏）
                        let (lx, ly) = (x + cn.position.x, y + cn.position.y);
                        if ly >= container_rect.top { return None; } // 展开态：无缺口
                        Some(Rect::new(
                            lx - 4.0, ly,
                            lx + cn.measured_size.width + 4.0, ly + cn.measured_size.height,
                        ))
                    })
                } else { None };
                let focus_p = focus_progress.peek();
                let shape = shape.eval();
                draw_text_field_container(canvas, container_rect, variant, &shape, colors, &indicator_color.peek(), focus_p, cutout);
                // 支持文本：容器底部外侧 4dp
                if let Some(sv) = supporting {
                    draw_text_field_aux_text(
                        canvas,
                        sv.content.as_str(),
                        sv.font_size,
                        sv.font_weight,
                        sv.font_style,
                        sv.letter_spacing,
                        sv.line_height,
                        &sv.color,
                        (x + 16.0, container_rect.bottom + 4.0),
                        w - 32.0,
                    );
                }
            }
            // 自定义绘制（Slider 轨道/thumb 等动态图形——闭包以节点 rect 调用）
            ModifierElement::CustomDraw { f } => {
                f(canvas, rect);
            }
            // 图标绘制于内容区域（padding 内缩——叶子 padding 渲染偏移）
            ModifierElement::DrawIcon { spec } => {
                draw_icon(canvas, Rect::new(content_x, content_y, content_x + content_w, content_y + content_h), node.layout_direction, spec);
            }
            // 图片（Image 组件）：ContentScale + 对齐 + alpha + colorFilter + filterQuality
            ModifierElement::ImageContent { source, content_scale, alignment, alpha, color_filter, filter_quality } => {
                let alpha = alpha.eval();
                let color_filter = color_filter.as_ref().map(|filter| filter.eval());
                draw_image_content(
                    canvas,
                    Rect::new(content_x, content_y, content_x + content_w, content_y + content_h),
                    source,
                    *content_scale,
                    *alignment,
                    alpha,
                    color_filter.as_ref(),
                    *filter_quality,
                    node.layout_direction,
                );
            }
            el => {
                if let Some(tp) = render_modifier_element(
                    canvas,
                    el,
                    // `paint_rect`, not the node rect: it is inset by whatever
                    // `minimum_interactive_size` added to reach the 48 dp touch target.
                    paint_rect,
                    &mut last_background,
                    tf_radii,
                ) {
                    text = Some((tp.content, tp.font_size, tp.color, tp.max_lines, tp.align, tp.overflow, tp.font_weight, tp.font_style, tp.soft_wrap, tp.letter_spacing, tp.line_height));
                }
            }
        }
    }

    // Open draw nodes (exp/modifier-node): node chain runs after the enum chain
    // (P1-1 fix — nodes used to run before enums and were covered by them).
    // NOTE: chain order is not preserved — nodes paint uniformly after all enum
    // draws (incl. icon/image), before text. Background-layer approximation.
    for draw_node in node.modifier.draw_nodes() {
        draw_node.draw(canvas, rect);
    }
    // Wrapping draw nodes, before half (exp/draw-wrap-node): same background-layer
    // slot as DrawNode. The after half runs after children + ripple (see below).
    for wrap_node in node.modifier.draw_wrap_nodes() {
        wrap_node.draw_before(canvas, rect, &node.modifier);
    }

    // 内容模糊：saveLayer
    if let Some(r) = blur_radius {
        let mut paint = Paint::default();
        paint.set_image_filter(image_filters::blur((r, r), skia_safe::TileMode::Clamp, None, None));
        let rec = skia_safe::canvas::SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }

    // Clip：在绘制内容前设置裁剪区域
    // （转场 morph 优先：布局空间圆角在变换下精确落到插值矩形上——与
    // transiton-block 的屏幕空间 clip 等价，由线性保证；仅取其一时行为一致）
    let mut clipped = false;
    if let Some(rr) = tf_clip_rr {
        canvas.save();
        canvas.clip_rrect(rr, None, Some(false));
        clipped = true;
    } else if let Some(ref shape) = clip_shape {
        canvas.save();
        // Clipped at `paint_rect`, like the background, the border, the ripple, the focus ring and the
        // graphics layer: what the clip belongs to is the container a component declares inside its
        // touch target, not the target itself. Compose gets there by nesting
        // (`InteractiveComponentSize.kt:125-133` wraps an inner `.size(40)`), so a clip inside that
        // inner node is the 40 dp box; here the target IS the node, so `rect` clipped the 48 dp box
        // and a squeezed item — whose container the row steers inside it — was clipped loosely.
        match shape {
            crate::graphics::Shape::Rectangle => { canvas.clip_rect(paint_rect, None, Some(false)); }
            crate::graphics::Shape::RoundedRect { corner_radius } => {
                canvas.clip_rrect(RRect::new_rect_xy(paint_rect, *corner_radius, *corner_radius), None, Some(false));
            }
            crate::graphics::Shape::PillToRounded { corner_radius, t } => {
                let percent = paint_rect.width().min(paint_rect.height()) / 2.0;
                let r = percent + (*corner_radius - percent) * *t;
                canvas.clip_rrect(RRect::new_rect_xy(paint_rect, r, r), None, Some(false));
            }
            crate::graphics::Shape::TopRoundedRect { radius } => {
                let rr = RRect::new_rect_radii(paint_rect, &[
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(0.0, 0.0),
                    skia_safe::Vector::new(0.0, 0.0),
                ]);
                canvas.clip_rrect(rr, None, Some(false));
            }
            crate::graphics::Shape::RightRoundedRect { radius } => {
                canvas.clip_rrect(rrect_right_rounded(paint_rect, *radius), None, Some(false));
            }
            crate::graphics::Shape::LeftRoundedRect { radius } => {
                canvas.clip_rrect(rrect_left_rounded(paint_rect, *radius), None, Some(false));
            }
            crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
                canvas.clip_rrect(
                    rrect_corners(paint_rect, *top_left, *top_right, *bottom_right, *bottom_left),
                    None,
                    Some(false),
                );
            }
            crate::graphics::Shape::Pill => {
                let r = paint_rect.width().min(paint_rect.height()) / 2.0;
                canvas.clip_rrect(RRect::new_rect_xy(paint_rect, r, r), None, Some(false));
            }
            crate::graphics::Shape::Circle => {
                // Circle == Pill (see the fill path): percent-50 corners against
                // the box, not an ellipse.
                let r = paint_rect.width().min(paint_rect.height()) / 2.0;
                canvas.clip_rrect(RRect::new_rect_xy(paint_rect, r, r), None, Some(false));
            }
        }
        clipped = true;
    }

    if let Some((content, font_size, color, max_lines, align, overflow, font_weight, font_style, soft_wrap, letter_spacing, line_height)) = text {
        // 优先用测量阶段缓存的 Paragraph（避免重建）
        if let Some(para) = node.cached_paragraph.borrow_mut().as_mut() {
            // Only re-lay it out when the text WRAPS. A `soft_wrap(false)` paragraph was built at
            // its intrinsic width (`build_plain_paragraph`), and laying it out again at the node's
            // content width truncates it — which is what cut a button-group label to `Sta` while a
            // press narrowed its item, and exactly what `TextOverflow::Visible` exists to prevent.
            if soft_wrap {
                para.layout(content_w);
            }
            // Slack is how far the drawn paragraph can move inside its box. A WRAPPED paragraph
            // is clamped to the box, so a negative slack would push it out of it. An unwrapped one
            // was laid out at its intrinsic width and can be WIDER than the box — then centring or
            // right-aligning has to shift it the other way rather than pin it to the left edge,
            // which is what keeps a button-group label centred while a press narrows its item.
            let slack = if soft_wrap {
                (content_w - para.max_intrinsic_width()).max(0.0)
            } else {
                content_w - para.max_intrinsic_width()
            };
            let x_off = match align {
                crate::text::TextAlign::Left | crate::text::TextAlign::Justify => content_x,
                crate::text::TextAlign::Center => content_x + slack / 2.0,
                crate::text::TextAlign::Right => content_x + slack,
            };
            // 选中高亮
            if let Some(range) = node.registrar.borrow().as_ref().cloned().and_then(|reg| reg.selected_range(node.slot_key)) {
                debug_log!("[selection] render node={} range={}..{}", node.id, range.start, range.end);
                let rects: Vec<_> = if range.start < range.end {
                para.get_rects_for_range(range.start..range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max) } else { Vec::new() };
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                for tb in &rects {
                    canvas.draw_rect(skia_safe::Rect::new(x_off + tb.rect.left, content_y + tb.rect.top, x_off + tb.rect.right, content_y + tb.rect.bottom), &paint);
                }
            }
            para.paint(canvas, x_off, content_y);
            // 绘制光标（聚焦的 TextField 节点；色 = M3 cursor（primary/error））。
            // ⚠ 有选区（非零宽）时不显示——对齐 Compose：光标仅 collapsed
            // selection 时绘制
            let has_selection = node.registrar.borrow().as_ref()
                .and_then(|reg| reg.selected_range(node.slot_key))
                .map(|r| r.start < r.end)
                .unwrap_or(false);
            // 聚焦判定：优先组合期标记（text-field-v2 容器化——焦点在容器，
            // 输入子节点用 display_focused），回退节点自身 focused
            let focused = node.display_focused.get() || node.focused;
            // material3 draws its caret as `showCursor = enabled && !readOnly && …`;
            // `text_field_show_cursor` reads those two off the container's visual element along the parent
            // chain (the same walk the cursor colour below uses) and defaults to true for a bare field,
            // which has neither.
            if focused && !has_selection && crate::text::field::text_field_show_cursor(nodes, root_idx, idx) {
                if node.cursor_visible.get() {
                    // ⚠ cursor_color 在容器 TextFieldVisual（组合期解析 primary/
                    // error）——输入 leaf 无此元素，从 leaf 找会回退文本色。
                    // 沿 parent 链向上找容器（offset_mapping_for_node 同路径）
                    let cursor = crate::text::field::text_field_visual_color(nodes, root_idx, idx)
                        .unwrap_or(*color);
                    let mut cp = skia_safe::Paint::default();
                    cp.set_color(skia_safe::Color::from_argb(255, cursor.r, cursor.g, cursor.b));
                    cp.set_stroke_width(1.5);
                    // 空文本：无 glyph 可定位——光标画在内容起点（M3 光标高度
                    // 用行高近似；TextLayout 空特判内部 get_glyph_cluster_at(0)
                    // 同样失败）
                    if content.is_empty() {
                        let ch = font_size * 1.4;
                        canvas.draw_line(
                            skia_safe::Point::new(content_x, content_y),
                            skia_safe::Point::new(content_x, content_y + ch),
                            &cp,
                        );
                    } else {
                        // ⚠ length = 文本真实字节数（空文本为 0 → 空特判）——
                        // 映射表长度恒 ≥1（含 end-of-text 映射），传它空文本
                        // 时走主路径 get_by_right(0) 失败 → 光标不显示
                        let tl = crate::text::TextLayout::new(para, content.len());
                        // 光标索引是编辑偏移——经 OffsetMapping 转显示偏移
                        // （密码掩码/格式化输入显示文本 ≠ 编辑文本；
                        // TextFieldVisual 在容器——向上找）
                        let offset_mapping = crate::text::field::offset_mapping_for_node(nodes, root_idx, idx);
                        let idx = offset_mapping.as_ref()
                            .map(|m| m.original_to_transformed(node.cursor_index.get()))
                            .unwrap_or_else(|| node.cursor_index.get());
                        if let Some((cx, cy, ch)) = tl.get_cursor_position(idx) {
                            debug_log!("[render] cursor pos=({:.0},{:.0}) h={:.0}", cx, cy, ch);
                            canvas.draw_line(skia_safe::Point::new(x_off + cx, content_y + cy), skia_safe::Point::new(x_off + cx, content_y + cy + ch), &cp);
                        } else { debug_log!("[render] get_cursor_position returned None for idx={}", node.cursor_index.get()); }
                    }
                } else { debug_log!("[render] cursor_visible is false"); }
            }
            // IME 组合文本下划线（编辑偏移 → 显示偏移）
            if let Some(comp_range) = node.composing_range.borrow().as_ref() {
                if comp_range.start < comp_range.end {
                    let offset_mapping = crate::text::field::offset_mapping_for_node(nodes, root_idx, idx);
                    let (cs, ce) = offset_mapping.as_ref().map(|m| {
                        (m.original_to_transformed(comp_range.start), m.original_to_transformed(comp_range.end))
                    }).unwrap_or((comp_range.start, comp_range.end));
                    let rects = para.get_rects_for_range(cs..ce, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max);
                    let mut und_paint = skia_safe::Paint::default();
                    // Phase 4.2：组合期捕获的主题 primary（render 阶段 CompositionLocal
                    // 已退出，读 WiniaTheme::colors() 会得到默认主题）
                    let c = node.composing_color.get();
                    und_paint.set_color(skia_safe::Color::from_argb(c.a, c.r, c.g, c.b));
                    und_paint.set_stroke_width(1.0);
                    for tb in &rects {
                        let r = tb.rect;
                        canvas.draw_line(skia_safe::Point::new(x_off + r.left, content_y + r.bottom), skia_safe::Point::new(x_off + r.right, content_y + r.bottom), &und_paint);
                    }
                }
            }
        } else {
            draw_text_with_selection(canvas, content, font_size, color, font_weight, font_style, content_x, content_y, content_w, max_lines, align, overflow, soft_wrap, letter_spacing, line_height, node.registrar.borrow().as_ref().cloned(), node.slot_key);
        }
    }

    // ═══ 富文本（RichText）渲染 ═══
    if node.has_richtext_content {
        if let Some(para) = node.cached_paragraph.borrow_mut().as_mut() {
            para.layout(content_w);
            // 选中高亮
            if let Some(range) = node.registrar.borrow().as_ref().cloned().and_then(|reg| reg.selected_range(node.slot_key)) {
                let rects: Vec<_> = if range.start < range.end {
                para.get_rects_for_range(range.start..range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max) } else { Vec::new() };
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                for tb in &rects {
                    canvas.draw_rect(skia_safe::Rect::new(content_x + tb.rect.left, content_y + tb.rect.top, content_x + tb.rect.right, content_y + tb.rect.bottom), &paint);
                }
            }
            para.paint(canvas, content_x, content_y);
        }
    }

    // 焦点环：聚焦淡入、失焦淡出——透明度来自交互源 focus_indicator_alpha
    // （失焦后动画期间 alpha>0 继续绘制，实现平滑淡出）。
    // ⚠ TextField 容器（M3）不用焦点环——用指示线/边框色变化提示焦点
    let has_tf_container = node.modifier.elements().iter()
        .any(|el| matches!(el, ModifierElement::TextFieldVisual { .. }));
    // NoFocusRing：组件自绘焦点环（Slider 焦点环包围 thumb 而非整组件）
    let no_focus_ring = node.modifier.elements().iter()
        .any(|el| matches!(el, ModifierElement::NoFocusRing));
    let focus_alpha = node.modifier.focusable_interaction()
        .map(|src| src.focus_indicator_alpha_value())
        .unwrap_or(if node.focused { 1.0 } else { 0.0 });
    // Deferred: the ring sits OUTSIDE the node's own clip (see draw_focus), so it has to be painted
    // after the container clip is restored at the end of this pass. Painting it here clipped the whole
    // ring away on any component that carries Modifier::clip(shape) — IconButton and IconToggleButton
    // both do, and their focus ring was invisible.
    let mut pending_focus_ring = None;
    if !has_tf_container && !no_focus_ring && (node.focused || focus_alpha > 0.001) {
        // 焦点环形状跟随组件（最近 Background/Border/Clip 形状，回退矩形）；
        // 颜色由组件组合期从主题捕获（node.focus_color）
        let focus_shape = node.modifier.elements().iter().rev().find_map(|el| match el {
            ModifierElement::Background { shape, .. }
            | ModifierElement::BackgroundBrush { shape, .. }
            | ModifierElement::Border { shape, .. }
            | ModifierElement::Clip { shape } => Some(shape),
            _ => None,
        }).map(|shape| shape.eval()).unwrap_or(crate::graphics::Shape::Rectangle);
        // `paint_rect`, not `rect`: the ring hugs the container. Drawing the container's rounded shape on
        // the (possibly grown) node box put a squircle outside the control on the two smallest sizes.
        pending_focus_ring = Some((paint_rect, focus_shape, tf_radii, node.focus_color.get(), focus_alpha));
    }

    // Scroll clip + translate
    let mut scrolled = false;
    // Replayed for the focus ring below: the ring is painted after the scroll save is restored (it
    // must escape the container's clip), but it still has to ride the scroll translation and the
    // viewport clip.
    let mut scroll_replay: Option<(Rect, (f32, f32))> = None;
    if scroll_offset_v.is_some() || scroll_offset_h.is_some() {
        // clip 用 visible 尺寸，优先从 modifier 中提取 Size 或 FillMax 信息
        let mut cw = w;
        let mut ch = h;
        for el in node.modifier.elements() {
            match el {
                ModifierElement::Size { width, height } => {
                    if let crate::layout::SizeValue::Length(dw) = width {
                        if dw.is_fixed() { cw = dw.to_logical_px(); }
                    }
                    if let crate::layout::SizeValue::Length(dh) = height {
                        if dh.is_fixed() { ch = dh.to_logical_px(); }
                    }
                }
                ModifierElement::FillMaxWidth { .. } | ModifierElement::FillMaxSize { .. } => {
                    cw = w; // measured width = the filled share of the parent max width
                }
                ModifierElement::FillMaxHeight { .. } => {
                    ch = h;
                }
                _ => {}
            }
        }
        // scroll 容器的可视尺寸以测量期回写的 viewport 为准——measured_size
        // 是内容全高（apply_scroll_delta 的 max_offset 依赖它），直接用会把
        // 裁剪矩形拉到内容末端，滚动内容会越界绘制到后续兄弟（如 Scaffold
        // bottom bar）之上
        if node.scroll_viewport_height > 0.0 {
            ch = node.scroll_viewport_height;
        }
        if node.scroll_viewport_width > 0.0 {
            cw = node.scroll_viewport_width;
        }
        let mut clip_rect = Rect::new(x, y, x + cw, y + ch);
        // Compose's `clipScrollableContainer`: the main axis is clipped to the container, the cross
        // axis keeps MAX_SUPPORTED_ELEVATION (30 dp) so a shadow drawn on that axis is not cut.
        if let Some(vertical) = node.modifier.clip_scrollable_vertical() {
            let slack = 30.0 * crate::runtime::density::current_density().density;
            clip_rect = if vertical {
                Rect::new(x - slack, y, x + cw + slack, y + ch)
            } else {
                Rect::new(x, y - slack, x + cw, y + ch + slack)
            };
        }
        let mut dx = -scroll_offset_h.unwrap_or(0.0);
        let mut dy = -scroll_offset_v.unwrap_or(0.0);
        // Overscroll pulls the content past the edge; the clip above keeps it inside the viewport,
        // so the pull reads as the content shifting and the gap it is pulled from.
        if let Some(effect) = node.modifier.overscroll_effect() {
            if !node.scroll_reverse {
                let pull = effect.stretch();
                if scroll_offset_v.is_some() {
                    dy -= pull;
                }
                if scroll_offset_h.is_some() {
                    dx -= pull;
                }
            }
        }
        canvas.save();
        canvas.clip_rect(clip_rect, None, Some(false));
        canvas.translate((dx, dy));
        scrolled = true;
        scroll_replay = Some((clip_rect, (dx, dy)));
    }

    // 穿行子节点（backdrop 节点自身内容照常绘制在模糊层之上）。
    // PAINT 顺序 = 树顺序，除非有子节点设了 `Modifier::z_index`——那时按 z 稳定排序
    // （z 高的后画、盖住兄弟；命中按同一顺序倒序，见 `paint_order`）。未使用 z 的
    // 节点走原路径，不做任何额外工作。
    if node.children_have_z {
        for child in crate::layout::node::paint_order(nodes, &node.children) {
            render_pass1(nodes, root_idx, child, canvas, x, y, false, rootless);
        }
    } else {
        for &child in &node.children {
            render_pass1(nodes, root_idx, child, canvas, x, y, false, rootless);
        }
    }

    // Item exit ghosts belong to this container: drawing them here keeps the container's scroll
    // clip and translate (Lazy) and avoids the process-global store painting into other windows.
    if let Some(host) = &item_host {
        if !CAPTURING_ITEM.with(|c| c.get()) {
            draw_host_exits(canvas, host);
        }
    }

    // Inner shadow: painted ABOVE the content (Compose's appearance draws it after the shape fill
    // and its children) and clipped to the shape.
    for el in node.modifier.elements() {
        if let ModifierElement::InnerShadow { shape, color, blur, offset_x, offset_y } = el {
            let shape = shape.eval();
            draw_inner_shadow(canvas, paint_rect, &shape, color, *blur, *offset_x, *offset_y);
        }
    }

    // Ripple indication — above content, inside shape/scroll clipping. `paint_rect`, so the ripple
    // matches the container that was painted: a node grown to the 48 dp touch target paints its 40 dp
    // container, and a ripple spanning the node would spill outside it.
    draw_ripple(
        node,
        canvas,
        paint_rect.x(),
        paint_rect.y(),
        paint_rect.width(),
        paint_rect.height(),
        rect.x(),
        rect.y(),
    );

    // Wrapping draw nodes, after half (exp/draw-wrap-node): above children and
    // above ripple, inside the scroll translate (same stack as ripple — the node
    // follows scrolled content). This is the slot a future RippleNode migration
    // would use; the enum Ripple path stays untouched.
    for wrap_node in node.modifier.draw_wrap_nodes() {
        wrap_node.draw_after(canvas, rect, &node.modifier);
    }

    if scrolled {
        canvas.restore();
    }

    if clipped {
        canvas.restore();
    }

    // The focus ring, outside the container clip but still inside the scroll viewport and the
    // graphics-layer effects.
    if let Some((ring_rect, ring_shape, ring_radii, ring_color, ring_alpha)) = pending_focus_ring {
        let replayed = scroll_replay.is_some();
        if let Some((clip_rect, (dx, dy))) = scroll_replay {
            canvas.save();
            canvas.clip_rect(clip_rect, None, Some(false));
            canvas.translate((dx, dy));
        }
        draw_focus(canvas, ring_rect, &ring_shape, ring_radii, ring_color, ring_alpha, 2.0);
        if replayed {
            canvas.restore();
        }
    }

    if blur_radius.is_some() {
        canvas.restore();
    }

    if gl_saved {
        canvas.restore();
    }

    // Flight morph restores (LIFO: the outermost block restores last).
    if tf_layered {
        canvas.restore();
    }
    if tf_saved {
        canvas.restore();
    }
}

/// 水波纹（按旧版 D:\winia ripple.rs 的绘制方式）：
/// - 两层模型：背景层（hover/focus 状态层）= 节点中心圆（半径=对角线/2，
///   直径=对角线）；前景层 = 按压点实心圆（半径=对角线×progress）
/// - 前景层裁剪到背景的范围和形状：bounded → 节点形状；unbounded →
///   背景圆（直径=对角线）——波纹不会超出背景层
/// - hover/focus 状态层透明度动画值；每层扩散/淡出由动画系统驱动
/// `x, y, w, h` describe the CONTAINER — the node's box minus its own minimum-interactive inset — which is
/// what the halo and the clip use. `node_x, node_y` are the node's own origin: the recorded press point is
/// node-local, so the spreading foreground must be drawn from there or it lands inset by that same amount.
pub(super) fn draw_ripple(
    node: &LayoutNode,
    canvas: &Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    node_x: f32,
    node_y: f32,
) {
    for el in node.modifier.elements() {
        let ModifierElement::Ripple { source, color, bounded, shape, state_layer_diameter } = el else { continue };
        // The colour and the halo diameter are carriers: evaluated once for this paint, then shared by
        // the background layer and every spreading foreground layer.
        let color = color.eval();

        let diagonal = (w * w + h * h).sqrt();
        // The BACKGROUND state layer is winia's own two-layer model: a circle at the node's centre whose
        // diameter is the node's DIAGONAL — unless the component named one (Compose's `ripple(bounded, radius)`,
        // which material3's checkbox and radio use for their 40 dp `StateLayerSize`, smaller than their 48 dp
        // node). The spreading FOREGROUND layer is unaffected by it: its radius always grows to the node's
        // diagonal, so a press near a corner still covers the control.
        let layer_radius = state_layer_diameter
            .as_ref()
            .map(|d| d.eval() / 2.0)
            .unwrap_or(diagonal / 2.0);
        let rect = Rect::new(x, y, x + w, y + h);

        // bounded：裁剪到波纹形状——显式 shape（Button 传入容器 shape——
        // Outlined/Text 无背景元素时仍按胶囊裁剪）优先；否则从链上最近的
        // Background/Border 推断（通用 clickable + ripple 保持可用）
        let mut clipped = false;
        if *bounded {
            let clip_shape = shape.clone().map(|shape| shape.eval()).or_else(|| {
                node.modifier.elements().iter().rev().find_map(|el| match el {
                    ModifierElement::Background { shape, .. }
                    | ModifierElement::BackgroundBrush { shape, .. }
                    | ModifierElement::Border { shape, .. } => Some(shape.eval()),
                    _ => None,
                })
            });
            canvas.save();
            clipped = true;
            match clip_shape {
                Some(crate::graphics::Shape::RoundedRect { corner_radius }) => {
                    canvas.clip_rrect(
                        skia_safe::RRect::new_rect_xy(rect, corner_radius, corner_radius),
                        None,
                        Some(false),
                    );
                }
                Some(crate::graphics::Shape::TopRoundedRect { radius }) => {
                    let rr = skia_safe::RRect::new_rect_radii(rect, &[
                        skia_safe::Vector::new(radius, radius),
                        skia_safe::Vector::new(radius, radius),
                        skia_safe::Vector::new(0.0, 0.0),
                        skia_safe::Vector::new(0.0, 0.0),
                    ]);
                    canvas.clip_rrect(rr, None, Some(false));
                }
                Some(crate::graphics::Shape::Pill) => {
                    let r = rect.width().min(rect.height()) / 2.0;
                    canvas.clip_rrect(
                        skia_safe::RRect::new_rect_xy(rect, r, r),
                        None,
                        Some(false),
                    );
                }
                Some(crate::graphics::Shape::Circle) => {
                    // Circle == Pill (see the fill path): percent-50 corners, so a
                    // non-square box is a stadium, not an ellipse.
                    let r = rect.width().min(rect.height()) / 2.0;
                    canvas.clip_rrect(RRect::new_rect_xy(rect, r, r), None, Some(false));
                }
                // The press morph's shape. WITHOUT this arm the catch-all below clipped the
                // ripple to the full RECT, so while a Button morphed its corner the ripple kept
                // painting the square corners the container had already given up.
                Some(crate::graphics::Shape::PillToRounded { corner_radius, t }) => {
                    let percent = rect.width().min(rect.height()) / 2.0;
                    let r = percent + (corner_radius - percent) * t;
                    canvas.clip_rrect(RRect::new_rect_xy(rect, r, r), None, Some(false));
                }
                Some(crate::graphics::Shape::RightRoundedRect { radius }) => {
                    canvas.clip_rrect(rrect_right_rounded(rect, radius), None, Some(false));
                }
                Some(crate::graphics::Shape::LeftRoundedRect { radius }) => {
                    canvas.clip_rrect(rrect_left_rounded(rect, radius), None, Some(false));
                }
                Some(crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left }) => {
                    canvas.clip_rrect(
                        rrect_corners(rect, top_left, top_right, bottom_right, bottom_left),
                        None,
                        Some(false),
                    );
                }
                // NO wildcard arm here on purpose, and this is the guard that was missing: a
                // `_ =>` clipped every shape it did not recognise to the full RECT, so when
                // `PillToRounded` was added for the press morph the ripple silently went on
                // painting the square corners the container had already given up. Listing the
                // two cases that really mean "no rounding" turns the next new `Shape` into a
                // compile error at this line instead.
                Some(crate::graphics::Shape::Rectangle) | None => {
                    canvas.clip_rect(rect, None, Some(false));
                }
            }
        } else {
            // unbounded: the foreground layer is CLIPPED to the background circle — winia's two-layer rule, and
            // the missing clip is what made a spread wider than its own halo appear. The background circle is
            // the halo (`state_layer_diameter` when the component named one, the node's diagonal otherwise).
            let r = layer_radius;
            canvas.save();
            clipped = true;
            canvas.clip_rrect(
                skia_safe::RRect::new_oval(Rect::from_xywh(
                    x + w / 2.0 - r,
                    y + h / 2.0 - r,
                    r * 2.0,
                    r * 2.0,
                )),
                None,
                Some(false),
            );
        }

        // State layer: a filled circle at the node's centre. Its radius is the explicit one when the ripple
        // named it (`ripple(bounded, radius)`), otherwise the node's diagonal (the original behaviour).
        let state_alpha = source.hover_opacity_value() + source.focus_opacity_value();
        if state_alpha > 0.0 {
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                (color.a as f32 * state_alpha) as u8,
                color.r,
                color.g,
                color.b,
            ));
            canvas.draw_circle(
                skia_safe::Point::new(x + w / 2.0, y + h / 2.0),
                layer_radius,
                &paint,
            );
        }

        // ── 前景波纹层：按压点实心圆（半径=对角线×progress——旧版 draw_circle；
        //    直径=2×对角线——按压点靠角落时圆足以覆盖整个组件）──
        // 中心存的是节点本地坐标（press_interaction_down 已从场景坐标换算）；
        // 画布坐标 = 布局原点 + 本地坐标——祖先 scroll translate 与自身
        // graphics_layer 变换都已作用在画布上，波纹视觉位置自动跟随节点
        // （按下后滚动/变换动画中不脱离按钮）。
        for layer in source.ripple_layers() {
            let progress = layer.progress.peek();
            let opacity = layer.opacity.peek();
            // Foreground radius = the background layer's diameter x progress, i.e. the foreground grows until
            // it fills the background circle; the clip above then holds it there. With a derived halo that
            // diameter is the node's diagonal (the original behaviour), with a named 40 dp halo it is 40 dp.
            let r = layer_radius * 2.0 * progress;
            if r <= 0.0 || opacity <= 0.0 {
                continue;
            }
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                (color.a as f32 * opacity) as u8,
                color.r,
                color.g,
                color.b,
            ));
            // The press point is node-local, so the foreground spreads from the NODE origin, not the
            // container's — using the container's put the whole layer inset by the same offset.
            canvas.draw_circle(
                skia_safe::Point::new(node_x + layer.center.0, node_y + layer.center.1),
                r,
                &paint,
            );
        }

        if clipped {
            canvas.restore();
        }
    }
}

