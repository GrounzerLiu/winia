use super::*;

// ── 背景模糊（BackdropBlur）──

/// 背景模糊采样区：blur sigma = radius（3σ 采样范围），快照扩展必须覆盖
/// 3σ 才不触及 snapshot 边界（Clamp 重复像素产生边缘条纹）。
const BACKDROP_BLUR_MARGIN: f32 = 3.0;

/// 节点背景模糊——参考 v1 `item.rs` 实现，关键点：
/// 1. **物理像素 snapshot**：节点四角经当前画布矩阵（含全局 HiDPI scale、
///    祖先 scroll/overlay 平移）映射为屏幕物理坐标，取 AABB + margin 扩展，
///    clamp 到 surface 边界（跨顶/左边缘时以相交原点锚定，防内容偏移；
///    仅用裸逻辑坐标 ×sf 会在滚动容器/overlay 中错位）
/// 2. **CropRect 限定滤镜输出** = 快照尺寸（防滤镜越界处理）
/// 3. **clip 到节点矩形**：只显示节点内部，边缘带不参与合成（无白雾晕开）
/// 4. **draw_image 画回**：逆矩阵把快照原点（物理像素）映射回画布逻辑坐标，
///    scale(1/矩阵缩放) —— 快照像素与物理像素严格 1:1，无采样缩放，
///    移动时边缘不闪烁
/// 语义：在节点自身内容（背景/文本/子节点）绘制**之前**执行——snapshot
/// 只含"位于其下"的已画内容（祖先 + 前面的兄弟），对齐 Compose。
/// 已知限制（与 v1 一致）：blur 绘制在 graphicsLayer 变换/alpha 之前——
/// 节点自身 alpha/scale/rotation 不作用于模糊层；旋转祖先下画回仅
/// translate+scale 为近似。这些场景未在 demo 覆盖。
pub(super) fn draw_backdrop_blur(
    canvas: &Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let margin = radius * BACKDROP_BLUR_MARGIN;

    // 节点屏幕物理位置：当前画布矩阵映射（含全局 sf 与祖先 scroll/gl/overlay）
    let m = canvas.local_to_device_as_3x3();
    let corners = [
        m.map_point((x, y)),
        m.map_point((x + w, y)),
        m.map_point((x, y + h)),
        m.map_point((x + w, y + h)),
    ];
    let (sx0, sy0) = corners.iter().fold((f32::MAX, f32::MAX), |acc, p| {
        (acc.0.min(p.x), acc.1.min(p.y))
    });
    let (sx1, sy1) = corners.iter().fold((f32::MIN, f32::MIN), |acc, p| {
        (acc.0.max(p.x), acc.1.max(p.y))
    });
    if !sx1.is_finite() || !sy1.is_finite() {
        return;
    }
    // 物理像素边界（floor/ceil——截断会丢边缘像素），并 clamp 到 surface：
    // `image_snapshot_with_bounds` 会与 surface 相交——若节点（或其 3σ
    // 采样区）跨出顶/左边缘，返回图像锚定在相交原点 (0,0)，画回若按未
    // clamp 的 (left,top) 锚定会整体偏移 (-left,-top) 物理像素（回归：
    // 旧两阶段实现有 `.max(0.0)`）。clamp 后以 clamp 原点锚定即对齐。
    let Some(surface) = (unsafe { canvas.surface() }) else { return; };
    let (sw_s, sh_s) = (surface.width(), surface.height());
    let Some(bounds) = backdrop_snapshot_irect(sx0, sy0, sx1, sy1, margin, sw_s, sh_s) else {
        return;
    };
    let (left, top, right, bottom) = (bounds.left, bounds.top, bounds.right, bounds.bottom);

    let Some(snap) = surface_snapshot(canvas, IRect::from_ltrb(left, top, right, bottom)) else {
        return;
    };
    let (sw, sh) = {
        let info = snap.image_info();
        (info.width(), info.height())
    };
    if sw <= 0 || sh <= 0 {
        return;
    }

    // CropRect 限定模糊输出 = 快照范围（参考 v1：防滤镜对越界区域采样）
    let mut paint = Paint::default();
    paint.set_image_filter(image_filters::blur(
        (radius, radius),
        skia_safe::TileMode::Clamp,
        None,
        image_filters::CropRect::from(Rect::from_wh(sw as f32, sh as f32)),
    ));

    // 画回：快照原点物理 (left,top) → 画布逻辑坐标（逆矩阵）
    let Some(inv) = m.invert() else { return };
    let origin = inv.map_point((left as f32, top as f32));
    // 矩阵缩放（无旋转/斜切时直接取对角线；否则均匀 scale = 1/√|det| 近似）
    let (rc00, rc01, rc10, rc11) = (m.rc(0, 0), m.rc(0, 1), m.rc(1, 0), m.rc(1, 1));
    let det = rc00 * rc11 - rc01 * rc10;
    if !det.is_finite() || det.abs() < 1e-9 {
        return;
    }
    let (sc_x, sc_y) = if rc01.abs() < 1e-4 && rc10.abs() < 1e-4 && rc00.abs() > 1e-4 && rc11.abs() > 1e-4 {
        (1.0 / rc00, 1.0 / rc11)
    } else {
        let s = 1.0 / det.abs().sqrt();
        (s, s)
    };

    canvas.save();
    // clip 到节点矩形（逻辑坐标——被画布矩阵自动变换）
    canvas.clip_rect(Rect::from_xywh(x, y, w, h), None, None);
    canvas.translate((origin.x, origin.y));
    canvas.scale((sc_x, sc_y));
    canvas.draw_image(&snap, skia_safe::Point::new(0.0, 0.0), Some(&paint));
    canvas.restore();
}

/// 快照物理像素边界：AABB + margin 扩展，floor/ceil 取整后 clamp 到
/// surface 范围。返回 None 表示与 surface 无交集（无需模糊）。
pub(super) fn backdrop_snapshot_irect(
    sx0: f32,
    sy0: f32,
    sx1: f32,
    sy1: f32,
    margin: f32,
    sw_s: i32,
    sh_s: i32,
) -> Option<IRect> {
    let left = ((sx0 - margin).floor() as i32).clamp(0, sw_s);
    let top = ((sy0 - margin).floor() as i32).clamp(0, sh_s);
    let right = ((sx1 + margin).ceil() as i32).clamp(0, sw_s);
    let bottom = ((sy1 + margin).ceil() as i32).clamp(0, sh_s);
    if right <= left || bottom <= top {
        None
    } else {
        Some(IRect::from_ltrb(left, top, right, bottom))
    }
}


pub(super) fn surface_snapshot(canvas: &Canvas, bounds: skia_safe::IRect) -> Option<skia_safe::Image> {
    let surface = unsafe { canvas.surface() }?;
    let mut surface = surface.clone();
    surface.image_snapshot_with_bounds(bounds)
}
