//! Inline drawables — pictures and SVG inside a run of text.
//!
//! A rich-text scope inserts one as a U+FFFC placeholder (`ParagraphBuilder::add_placeholder`),
//! the paragraph keeps it, and `Paragraph::paint` asks Skia for `get_rects_for_placeholders()` to
//! find where each one goes.

use skia_safe::{Canvas, Data, FontMgr, Paint, Rect};
use skia_safe::canvas::SrcRectConstraint;
use skia_safe::svg;
use std::fmt::{self, Debug};
use std::sync::Arc;

/// What an inline element has to do: draw itself, and say how big it is.
///
/// Implementors must be `Send + Sync`: a drawable is shared as `Arc<dyn InlineDrawable>` and can
/// be touched from another thread while the layout is being measured.
pub trait InlineDrawable: Send + Sync {
    /// Draw at the given origin.
    fn draw(&self, canvas: &Canvas, x: f32, y: f32);
    /// The element's size, `(width, height)` in logical pixels.
    fn size(&self) -> (f32, f32);
}

// ── ImageDrawable ──

/// An inline picture backed by a `skia_safe::Image`.
pub struct ImageDrawable {
    image: skia_safe::Image,
    width: f32,
    height: f32,
}

impl Debug for ImageDrawable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageDrawable")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl ImageDrawable {
    /// From a Skia image, scaled to the given size.
    pub fn from_image(image: skia_safe::Image, width: f32, height: f32) -> Self {
        Self { image, width, height }
    }

    /// Decoded from PNG/JPEG bytes and scaled to the given size.
    pub fn from_bytes(data: &[u8], width: f32, height: f32) -> Option<Self> {
        let image = skia_safe::Image::from_encoded(Data::new_copy(data))?;
        Some(Self::from_image(image, width, height))
    }
}

impl InlineDrawable for ImageDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        let src = Rect::new(0.0, 0.0, self.image.width() as f32, self.image.height() as f32);
        let dst = Rect::new(x, y, x + self.width, y + self.height);
        canvas.draw_image_rect(&self.image, Some((&src, SrcRectConstraint::Fast)), &dst, &Paint::default());
    }

    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
}

// ── SvgDrawable ──

/// An inline SVG backed by a `skia_safe::svg::Dom`.
///
/// Interior mutability comes from a `Mutex`, because `set_container_size` needs `&mut self` while
/// the drawable is shared behind an `Arc`. `svg::Dom`'s own handle is only conditionally
/// thread-safe, so the `Send`/`Sync` impls are still `unsafe` — but the lock is what makes them
/// true: two threads drawing at once serialise instead of racing on the DOM.
pub struct SvgDrawable {
    dom: parking_lot::Mutex<svg::Dom>,
    width: f32,
    height: f32,
}

// SAFETY: skia's `svg::Dom` is neither `Send` nor `Sync` (an `RCHandle` has no auto impl), so
// these markers are still ours to justify. The interior mutability that used to be a `RefCell` is
// now a `Mutex`, which is what makes the `Sync` claim TRUE: two threads drawing the same drawable
// take turns instead of both flipping one non-atomic borrow flag (that was UB, not a runtime
// check). The DOM is only ever touched while the lock is held, and `render` takes `&self`.
unsafe impl Send for SvgDrawable {}
unsafe impl Sync for SvgDrawable {}

impl Debug for SvgDrawable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SvgDrawable")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl SvgDrawable {
    /// Loaded from SVG bytes and scaled to the given size.
    pub fn from_bytes(data: &[u8], width: f32, height: f32) -> Option<Self> {
        let mut dom = svg::Dom::from_bytes(data, FontMgr::default()).ok()?;
        dom.set_container_size((width, height));
        Some(Self { dom: parking_lot::Mutex::new(dom), width, height })
    }

    /// Loaded from an SVG string and scaled to the given size.
    pub fn from_str(svg_str: &str, width: f32, height: f32) -> Option<Self> {
        let mut dom = svg::Dom::from_str(svg_str, FontMgr::default()).ok()?;
        dom.set_container_size((width, height));
        Some(Self { dom: parking_lot::Mutex::new(dom), width, height })
    }
}

impl InlineDrawable for SvgDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        canvas.save();
        canvas.translate((x, y));
        // `render` takes `&self`, but the lock is what serialises two windows drawing one SVG.
        // parking_lot's guard has no poisoning, so a panic in one painter cannot brick the icon.
        self.dom.lock().render(canvas);
        canvas.restore();
    }

    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
}

/// A convenience constructor for `Arc<dyn InlineDrawable>`.
impl From<ImageDrawable> for Arc<dyn InlineDrawable> {
    fn from(d: ImageDrawable) -> Self {
        Arc::new(d)
    }
}

impl From<SvgDrawable> for Arc<dyn InlineDrawable> {
    fn from(d: SvgDrawable) -> Self {
        Arc::new(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='#000'/></svg>";

    /// Two threads drawing one `SvgDrawable` must take turns, not race the DOM.
    ///
    /// The field used to be a `RefCell`, which is not atomic: the `unsafe impl Sync` let two
    /// threads trip the same borrow flag (UB). A `Mutex` serialises them instead, so this test
    /// completes instead of panicking with "already borrowed" or corrupting the flag.
    #[test]
    fn an_svg_drawable_can_be_drawn_from_two_threads() {
        use std::sync::Arc;
        let drawable: Arc<dyn InlineDrawable> = Arc::new(SvgDrawable::from_str(SQUARE, 10.0, 10.0).expect("svg parses"));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let d = drawable.clone();
            handles.push(std::thread::spawn(move || {
                let mut surface = skia_safe::surfaces::raster_n32_premul((20, 20)).expect("surface");
                for _ in 0..50 {
                    d.draw(surface.canvas(), 0.0, 0.0);
                }
            }));
        }
        for h in handles {
            h.join().expect("no thread may panic while drawing");
        }
    }
}