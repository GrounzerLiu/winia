use super::*;

// Image scaling — how a source fills the box it is drawn into.

/// How content scales into the box (Compose's `ContentScale`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentScale {
    /// Original size, no scaling (Compose `ContentScale.None`); anything outside the bounds is up to the clip.
    None,
    /// Fit inside the bounds, preserving the aspect ratio (the default; Compose `ContentScale.Fit`).
    Fit,
    /// Cover the bounds, preserving the aspect ratio and cropping the overflow (Compose `ContentScale.Crop`).
    Crop,
    /// Preserve the aspect ratio without exceeding the bounds (shrink, never enlarge; Compose `ContentScale.Inside`).
    Inside,
    /// Fill the width; the height follows the aspect ratio and may overflow (Compose `ContentScale.FillWidth`).
    FillWidth,
    /// Fill the height; the width follows the aspect ratio and may overflow (Compose `ContentScale.FillHeight`).
    FillHeight,
    /// Stretch both axes independently to the bounds, ignoring the aspect ratio (Compose `ContentScale.FillBounds`).
    /// NOTE: this is what winia's `None` used to be before it was aligned with Compose;
    /// the shared-element flight path needs the explicit member, because FillBounds is the
    /// behaviour it has to be able to reproduce.
    FillBounds,
}


/// Where the image content sits inside the bounds (Compose's nine `Alignment` values).
/// `Start`/`End` mirror with the layout direction, so `Start` is on the right under RTL.
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
    // Scaling goes through ContentScale::scale_factors, the single source of truth shared by Image
    // and the shared-element flight.
    let (fsx, fsy) = scale.scale_factors((iw, ih), (rect.width(), rect.height()));
    let (w, h) = (iw * fsx, ih * fsy);
    // Alignment offset (0.0 / 0.5 / 1.0; Start and End swap under RTL).
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
