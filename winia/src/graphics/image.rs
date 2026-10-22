use super::*;

// ─────────────────────────────────────────────────────────────────────────
// Icon 组件
// ─────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────
// Image scaling — how a source fills the box it is drawn into
// ─────────────────────────────────────────────────────────────────────────

/// 内容缩放模式（对标 Compose `ContentScale`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentScale {
    /// 原尺寸（不缩放——对标 ContentScale.None；超出 bounds 的部分由裁剪决定）
    None,
    /// 完整放入 bounds（保持比例——默认，对标 ContentScale.Fit）
    Fit,
    /// 覆盖 bounds（保持比例，超出部分裁剪——对标 ContentScale.Crop）
    Crop,
    /// 保持比例且不超过 bounds（缩小不放大——对标 ContentScale.Inside）
    Inside,
    /// 宽填满 bounds，高按比例（可超出——对标 ContentScale.FillWidth）
    FillWidth,
    /// 高填满 bounds，宽按比例（可超出——对标 ContentScale.FillHeight）
    FillHeight,
    /// 两轴独立拉伸到 bounds（不保持比例——对标 ContentScale.FillBounds）。
    /// NOTE: this is what winia's `None` used to be before it was aligned with Compose;
    /// the shared-element flight path needs the explicit member, because FillBounds is the
    /// behaviour it has to be able to reproduce.
    FillBounds,
}


// ── Image ──

/// 图片内容在 bounds 内的对齐（对标 Compose `Alignment` 9 值；
/// Start/End 随布局方向镜像——RTL 下 Start 在右）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageAlignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

/// How a source of `iw` × `ih` lands inside `rect`: the scale and the alignment offset, in one
/// place because the renderer and the measure pipeline must agree.
pub(crate) fn content_scale_rect(
    scale: ContentScale,
    rect: Rect,
    iw: f32,
    ih: f32,
    alignment: ImageAlignment,
    rtl: bool,
) -> Rect {
    if iw <= 0.0 || ih <= 0.0 {
        return rect;
    }
    // 缩放：走 `ContentScale::scale_factors` 这一处唯一事实来源（Image 与共享元素飞行共用）
    let (fsx, fsy) = scale.scale_factors((iw, ih), (rect.width(), rect.height()));
    let (w, h) = (iw * fsx, ih * fsy);
    // 对齐偏移（0.0/0.5/1.0；RTL 时 Start↔End 镜像）
    let sx = match alignment {
        ImageAlignment::TopStart | ImageAlignment::CenterStart | ImageAlignment::BottomStart => {
            if rtl { 1.0 } else { 0.0 }
        }
        ImageAlignment::TopCenter | ImageAlignment::Center | ImageAlignment::BottomCenter => 0.5,
        ImageAlignment::TopEnd | ImageAlignment::CenterEnd | ImageAlignment::BottomEnd => {
            if rtl { 0.0 } else { 1.0 }
        }
    };
    let sy = match alignment {
        ImageAlignment::TopStart | ImageAlignment::TopCenter | ImageAlignment::TopEnd => 0.0,
        ImageAlignment::CenterStart | ImageAlignment::Center | ImageAlignment::CenterEnd => 0.5,
        ImageAlignment::BottomStart | ImageAlignment::BottomCenter | ImageAlignment::BottomEnd => 1.0,
    };
    Rect::from_xywh(
        rect.left + (rect.width() - w) * sx,
        rect.top + (rect.height() - h) * sy,
        w,
        h,
    )
}
