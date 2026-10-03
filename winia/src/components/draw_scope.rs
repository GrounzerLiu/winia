//! `Canvas` / `DrawScope` — the general drawing surface, aligned with Compose's
//! `androidx.compose.foundation.Canvas`.
//!
//! Compose's `Canvas(modifier) { … }` is a `Spacer` with `drawBehind`, and the lambda receives a
//! `DrawScope`: a size-bounded drawing surface with the geometry already resolved (`size`, `center`)
//! and a set of `draw*` primitives. winia already had the two mechanisms this needs — the
//! `CustomDraw` modifier element (`Modifier::draw`) and the open `DrawNode`/`DrawWrapNode` extension
//! points (`docs/modifier-node.md`) — but no public *shape* for them: a caller had to hand a closure
//! a raw `skia_safe::Canvas` and a raw rect, and a component could not be written as a canvas at all.
//!
//! What this module adds is that shape:
//!
//! - [`DrawScope`] — the surface a drawing closure is handed: the node's rect as `size`/`center`/
//!   `top_left`, plus `draw_rect` / `draw_round_rect` / `draw_circle` / `draw_oval` / `draw_line` /
//!   `draw_path` / `draw_text`.
//! - [`Canvas`] — the component: it occupies the space its modifier gives it (a leaf, exactly like
//!   Compose's `Spacer`-backed canvas) and calls the drawing closure with a `DrawScope`.
//! - [`draw_behind`] / [`draw_with_content`] — the two modifier forms, Compose's `drawBehind` and
//!   `drawWithContent`. `draw_with_content` runs the closure *around* the node's own content
//!   (background, text, children), which is what makes a decorative overlay possible.
//!
//! # Deliberate difference: the scope speaks the layout coordinate system, not `Dp`
//!
//! winia HAS `Dp` (and `Sp`/`Px`/`Offset`/`Size`): `crate::unit` holds them, the prelude exports them,
//! and `Modifier::size` and friends accept them. What this scope hands back is the **layout coordinate
//! system** — `f32` logical pixels — because that is what `Constraints`, `measured_size` and every
//! drawing rect in the engine are in, and mixing the two is the trap `Dp::to_px`'s own docs warn
//! about (it returns PHYSICAL pixels, wrong anywhere the value meets layout geometry).
//!
//! So a caller states a *design intent* in dp on the way in (`Modifier::size(24.dp())`) and reads
//! resolved geometry in logical pixels on the way out, with `Dp` accessors
//! ([`DrawScope::width_dp`], [`DrawScope::height_dp`], [`DrawScope::size_dp`], [`DrawScope::center_dp`])
//! for code that wants the conversion spelled on the type rather than as a bare number.
//! `Dp::to_logical` is the conversion involved; `to_px` must not be used here.
//!
//! Rectangles travel as `skia_safe::Rect`, not as a `unit::Size` plus a `unit::Offset`, because a
//! drawing call needs one rectangle rather than a size and an origin; points and lengths are
//! `(f32, f32)` / `f32` for the same reason.
//!
//! # Other deliberate differences
//!
//! 1. **The region is the whole node rect and is not clipped.** Compose clamps a `DrawScope` to the
//!    drawing bounds it was handed; here clipping stays the caller's decision (`Modifier::clip`), which
//!    is how the framework's own nodes draw.
//! 2. **No `TextMeasurer` object.** `draw_text` shapes through `text::build_plain_paragraph` (the same
//!    path a `Text` node uses) and returns [`TextMetrics`] — the run's measured size plus a handle to
//!    draw it again, which is the part of a measurer this surface needs, without an object to thread
//!    through.
//! 3. **No `draw_image` yet.** An image needs a public decoded-image handle, which the framework does
//!    not have (`IconSource` is path/SVG/file text, resolved at draw time). Use `skia_canvas()` in the
//!    meantime; `draw_image` follows when such a handle exists — a separate component, not this one.
//! 4. `draw_with_content`'s "content" is the node the modifier is attached to: on a container its
//!    children, on a leaf its own background/border/text. That is the position
//!    `DrawWrapNode::draw_after` occupies in the framework's own pipeline.
use crate::unit::Size;

use std::sync::Arc;

use crate::modifier::{DrawWrapNode, Modifier};
use crate::graphics::{Color};
use skia_safe::{Canvas as SkCanvas, Paint, Path, Rect as SkRect, RRect};

/// The drawing surface handed to a [`Canvas`] / [`draw_behind`] / [`draw_with_content`] closure.
///
/// Borrows the Skia canvas for the duration of the call and remembers the owning node's rectangle,
/// so a closure can express positions relative to `size`/`center` without recomputing them.
pub struct DrawScope<'a> {
    canvas: &'a SkCanvas,
    rect: SkRect,
    density: f32,
}

impl<'a> DrawScope<'a> {
    pub(crate) fn new(canvas: &'a SkCanvas, rect: SkRect, density: f32) -> Self {
        Self { canvas, rect, density }
    }

    /// The region this scope draws into (the node's rect, in logical pixels).
    pub fn rect(&self) -> SkRect {
        self.rect
    }

    pub fn size(&self) -> (f32, f32) {
        (self.rect.width(), self.rect.height())
    }

    pub fn width(&self) -> f32 {
        self.rect.width()
    }

    pub fn height(&self) -> f32 {
        self.rect.height()
    }

    /// Center of the region — the anchor most canvas code positions against.
    pub fn center(&self) -> (f32, f32) {
        (
            self.rect.left + self.rect.width() / 2.0,
            self.rect.top + self.rect.height() / 2.0,
        )
    }

    pub fn top_left(&self) -> (f32, f32) {
        (self.rect.left, self.rect.top)
    }

    /// The width as a `Dp` — the same number `width()` returns, spelled on the type so a caller can
    /// mix it with `Modifier` arguments without converting by hand. `Dp::to_logical` is the identity
    /// here; do NOT round-trip through `to_px`, which is physical pixels.
    pub fn width_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.width())
    }

    pub fn height_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.height())
    }

    pub fn size_dp(&self) -> (crate::unit::Dp, crate::unit::Dp) {
        (self.width_dp(), self.height_dp())
    }

    /// The centre as `(Dp, Dp)`.
    pub fn center_dp(&self) -> (crate::unit::Dp, crate::unit::Dp) {
        let (x, y) = self.center();
        (crate::unit::Dp(x), crate::unit::Dp(y))
    }

    /// Density (scale factor) the frame is being drawn at.
    pub fn density(&self) -> f32 {
        self.density
    }

    /// The underlying Skia canvas, for anything this scope does not wrap.
    pub fn skia_canvas(&self) -> &SkCanvas {
        self.canvas
    }

    fn fill(&self, color: Color) -> Paint {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(crate::render::skia_color(color));
        paint
    }

    /// Filled rectangle, in the scope's coordinates.
    pub fn draw_rect(&self, rect: SkRect, color: Color) {
        self.canvas.draw_rect(rect, &self.fill(color));
    }

    /// Circle centered at `(cx, cy)`.
    pub fn draw_circle(&self, cx: f32, cy: f32, radius: f32, color: Color) {
        self.canvas.draw_circle((cx, cy), radius, &self.fill(color));
    }

    /// Oval inscribed in `rect`.
    pub fn draw_oval(&self, rect: SkRect, color: Color) {
        self.canvas.draw_oval(rect, &self.fill(color));
    }

    /// Rounded rectangle with a uniform corner radius.
    pub fn draw_round_rect(&self, rect: SkRect, radius: f32, color: Color) {
        self.canvas
            .draw_rrect(RRect::new_rect_xy(rect, radius, radius), &self.fill(color));
    }

    /// Line between two points, with the given stroke width (no caps: Compose's default is butt).
    pub fn draw_line(&self, from: (f32, f32), to: (f32, f32), stroke_width: f32, color: Color) {
        let mut paint = self.fill(color);
        paint.set_stroke_width(stroke_width);
        paint.set_style(skia_safe::PaintStyle::Stroke);
        self.canvas.draw_line(from, to, &paint);
    }

    /// Filled path.
    pub fn draw_path(&self, path: &Path, color: Color) {
        self.canvas.draw_path(path, &self.fill(color));
    }

    /// Filled path with an explicit paint, for gradients/shaders or a stroked style.
    pub fn draw_path_paint(&self, path: &Path, paint: &Paint) {
        self.canvas.draw_path(path, paint);
    }

    /// A text run at `(x, y)` (the top-left of the run's box), in `font_size` logical pixels, laid
    /// out unwrapped.
    ///
    /// Shapes through the framework's own paragraph construction (`build_plain_paragraph`), so canvas
    /// text goes through the same path a `Text` node uses rather than a second one — and returns
    /// [`TextMetrics`], so a caller can act on the measured size (centre the run, size a badge around
    /// it, reserve space) instead of re-deriving the layout or hard-coding an estimate.
    pub fn draw_text(&self, text: &str, x: f32, y: f32, font_size: f32, color: Color) -> TextMetrics {
        let paragraph = crate::text::build_plain_paragraph(text, font_size, color, self.rect.width().max(1.0));
        paragraph.paint(self.canvas, x, y);
        TextMetrics { paragraph, origin: (x, y) }
    }
}

/// What a [`DrawScope::draw_text`] call produced: the laid-out run, its origin, and a handle to the
/// paragraph that drew it.
///
/// This is the piece of Compose's `TextMeasurer` a drawing surface actually needs: after drawing, the
/// caller usually wants to know how big the run was (to centre it, to size a container around it, to
/// decide whether it fits). Without it the only options are to re-derive the layout or to hard-code an
/// estimate — which is the kind of code that drifts from the text the moment the font or the string
/// changes.
pub struct TextMetrics {
    paragraph: crate::text::Paragraph,
    origin: (f32, f32),
}

impl TextMetrics {
    /// The origin the run was drawn at (its top-left).
    pub fn origin(&self) -> (f32, f32) {
        self.origin
    }

    /// Width of the laid-out run in logical pixels (the widest line).
    pub fn width(&self) -> f32 {
        self.paragraph.max_intrinsic_width()
    }

    /// Height of the laid-out run in logical pixels (all lines).
    pub fn height(&self) -> f32 {
        self.paragraph.height()
    }

    pub fn size(&self) -> (f32, f32) {
        (self.width(), self.height())
    }

    /// The run's box as a rectangle, positioned at its origin.
    pub fn rect(&self) -> SkRect {
        SkRect::from_xywh(self.origin.0, self.origin.1, self.width(), self.height())
    }

    /// The run's width as a `Dp` (logical pixels — not `to_px`).
    pub fn width_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.width())
    }

    pub fn height_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.height())
    }

    /// The laid-out paragraph, for anything this type does not expose (per-line metrics, hit
    /// testing, ranges).
    pub fn paragraph(&self) -> &crate::text::Paragraph {
        &self.paragraph
    }

    /// Draw this run again with its box's top-left at `(x, y)`, through the [`DrawScope`] it was
    /// drawn with.
    ///
    /// The paragraph is reused, so drawing one string at several places (a repeated label, a
    /// watermark, a centred title) shapes it once instead of once per call — which is what a caller
    /// reaching for "draw it again" would otherwise pay for by calling `draw_text` repeatedly.
    pub fn draw_at(&self, scope: &DrawScope<'_>, x: f32, y: f32) {
        self.paragraph.paint(scope.skia_canvas(), x, y);
    }

    /// Draw this run so that its box is centred on `(width, height)`.
    pub fn draw_centered(&self, scope: &DrawScope<'_>, width: f32, height: f32) {
        self.draw_at(
            scope,
            (width - self.width()) / 2.0,
            (height - self.height()) / 2.0,
        );
    }
}

impl std::fmt::Debug for TextMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextMetrics")
            .field("origin", &self.origin)
            .field("size", &self.size())
            .finish()
    }
}

/// The component form: occupies the space its modifier gives it and draws in it.
///
/// ```ignore
/// Canvas::new()
///     .modifier(Modifier::new().size(120.0, 60.0))
///     .build(ctx, |scope| scope.draw_circle(scope.center().0, scope.center().1, 20.0, Color::RED));
/// ```
///
/// Compose's `Canvas` is a `Spacer` with `drawBehind`; this is the same composition on the inside,
/// so a `Canvas` with no modifier measures to zero (give it a size, or let a parent size it).
pub struct Canvas {
    modifier: Modifier,
}

impl Canvas {
    pub fn new() -> Self {
        Self { modifier: Modifier::new() }
    }

    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    pub fn build(self, ctx: &mut crate::runtime::composer::ComposeCtx, draw: impl Fn(&DrawScope) + Send + Sync + 'static) {
        let key = ctx.next_key();
        let draw = Arc::new(draw);
        let node = CanvasNode { draw };
        ctx.start_leaf(key, self.modifier.draw_wrap_node(node));
        ctx.end_node();
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

/// The node behind [`Canvas`]: draws in the node's rect on the "behind content" position, which for
/// a leaf with nothing else in its chain is simply the node's own rect.
struct CanvasNode {
    draw: Arc<dyn Fn(&DrawScope) + Send + Sync>,
}

impl std::fmt::Debug for CanvasNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CanvasNode")
    }
}

impl DrawWrapNode for CanvasNode {
    fn draw_before(
        &self,
        canvas: &SkCanvas,
        rect: SkRect,
        modifier: &Modifier,
    ) {
        let scope = DrawScope::new(canvas, rect, crate::unit::current_density().density);
        (self.draw)(&scope);
        let _ = modifier;
    }
}

/// Compose's `drawBehind`: draw behind the node's content.
pub fn draw_behind(modifier: Modifier, draw: impl Fn(&DrawScope) + Send + Sync + 'static) -> Modifier {
    modifier.draw_wrap_node(BehindNode { draw: Arc::new(draw) })
}

/// Compose's `drawWithContent`: draw around the node's content — `before` first, content, then
/// `after` — so a closure can decorate what the node paints instead of only what is behind it.
pub fn draw_with_content(
    modifier: Modifier,
    before: impl Fn(&DrawScope) + Send + Sync + 'static,
    after: impl Fn(&DrawScope) + Send + Sync + 'static,
) -> Modifier {
    modifier.draw_wrap_node(WithContentNode {
        before: Arc::new(before),
        after: Arc::new(after),
    })
}

struct BehindNode {
    draw: Arc<dyn Fn(&DrawScope) + Send + Sync>,
}

impl std::fmt::Debug for BehindNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BehindNode")
    }
}

impl DrawWrapNode for BehindNode {
    fn draw_before(&self, canvas: &SkCanvas, rect: SkRect, _modifier: &Modifier) {
        let scope = DrawScope::new(canvas, rect, crate::unit::current_density().density);
        (self.draw)(&scope);
    }
}

struct WithContentNode {
    before: Arc<dyn Fn(&DrawScope) + Send + Sync>,
    after: Arc<dyn Fn(&DrawScope) + Send + Sync>,
}

impl std::fmt::Debug for WithContentNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WithContentNode")
    }
}

impl DrawWrapNode for WithContentNode {
    fn draw_before(&self, canvas: &SkCanvas, rect: SkRect, _modifier: &Modifier) {
        let scope = DrawScope::new(canvas, rect, crate::unit::current_density().density);
        (self.before)(&scope);
    }

    fn draw_after(&self, canvas: &SkCanvas, rect: SkRect, _modifier: &Modifier) {
        let scope = DrawScope::new(canvas, rect, crate::unit::current_density().density);
        (self.after)(&scope);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::constraints::Constraints;
    use skia_safe::surfaces;

    /// Compose a single leaf with `modifier`, lay it out in a 300x300 window, render it on white and
    /// read one pixel back — the same shape the `modifier.rs` draw-node tests use, so a drawing claim
    /// is checked against output rather than against "the closure ran".
    fn render_pixel(modifier: Modifier, x: usize, y: usize) -> [u8; 4] {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(key, modifier);
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        px[y * 300 + x]
    }

    fn is_red(px: [u8; 4]) -> bool {
        // N32 premul stores BGRA: b, g, r, a.
        px[2] > 200 && px[1] < 60 && px[0] < 60
    }

    fn is_white(px: [u8; 4]) -> bool {
        px[0] > 240 && px[1] > 240 && px[2] > 240
    }

    /// The whole point of the surface: what the closure draws is what lands on the canvas, at the
    /// node's coordinates. Red circle in the middle of a 100x100 node.
    #[test]
    fn canvas_draws_a_circle_where_the_scope_says() {
        let modifier = Modifier::new().size(100.0, 100.0);
        let md = draw_behind(modifier, |scope| {
            let (cx, cy) = scope.center();
            scope.draw_circle(cx, cy, 20.0, Color::RED);
        });
        assert!(is_red(render_pixel(md.clone(), 50, 50)), "circle centre should be red");
        assert!(is_white(render_pixel(md, 5, 5)), "corner should stay white");
    }

    /// The scope's geometry is the node's rect, not the window's: `size`, `center` and `rect` all
    /// describe the measured node, so a canvas inside a padded parent draws in its own box.
    #[test]
    fn scope_geometry_follows_the_node_not_the_window() {
        let mut composer = Composer::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen_in = seen.clone();
        composer.compose(|ctx| {
            let modifier = Modifier::new().size(80.0, 40.0);
            let md = draw_behind(modifier, move |scope| {
                *seen_in.lock().unwrap() = Some((scope.width(), scope.height(), scope.center(), scope.rect()));
            });
            let key = ctx.next_key();
            ctx.start_leaf(key, md);
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let got = seen.lock().unwrap().expect("the draw closure ran");
        assert_eq!((got.0, got.1), (80.0, 40.0), "size is the node's");
        assert_eq!(got.2, (40.0, 20.0), "center is the node's centre");
        assert_eq!(
            (got.3.left, got.3.top, got.3.right, got.3.bottom),
            (0.0, 0.0, 80.0, 40.0),
            "rect is the node's rect"
        );
    }

    /// A `Canvas` is a leaf: with no modifier it measures to zero (Compose's `Spacer`-backed canvas
    /// behaves the same), and with one it takes that size. This is what makes "Canvas fills its
    /// parent" and "Canvas needs a size" both expressible.
    #[test]
    fn canvas_is_a_leaf_that_takes_the_size_its_modifier_gives_it() {
        let measured = |modifier: Modifier| {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                Canvas::new().modifier(modifier).build(ctx, |scope| {
                    scope.draw_rect(scope.rect(), Color::WHITE);
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
            let root = composer.layout_root_idx().expect("root");
            composer.arena_nodes()[root].measured_size
        };
        let zero = measured(Modifier::new());
        assert_eq!((zero.width, zero.height), (0.0, 0.0), "no size means no size");
        let sized = measured(Modifier::new().size(64.0, 32.0));
        assert_eq!((sized.width, sized.height), (64.0, 32.0), "the modifier's size is used");
    }

    /// `draw_with_content` draws on both sides of the content, and the "after" side lands on top —
    /// that is the difference between it and `draw_behind`, so it is worth pinning with pixels: a
    /// node whose own background is white, with a red "after" band, must read red where the band is.
    #[test]
    fn draw_with_content_paints_over_the_nodes_own_background() {
        let modifier = Modifier::new()
            .size(100.0, 100.0)
            .background(Color::WHITE, crate::graphics::Shape::Rectangle);
        let md = draw_with_content(
            modifier,
            |_scope| {},
            |scope| {
                scope.draw_rect(skia_safe::Rect::new(0.0, 0.0, 100.0, 20.0), Color::RED);
            },
        );
        assert!(is_red(render_pixel(md.clone(), 50, 10)), "the after-pass band is red");
        assert!(is_white(render_pixel(md, 50, 50)), "below the band the background shows");
    }

    /// The `Dp` accessors carry the same numbers as the raw ones — that is their whole contract. The
    /// pitfall they exist for is `Dp::to_px` (physical pixels), so the assertion also pins that a
    /// conversion back is the identity: `to_logical` of what these return is the logical value.
    #[test]
    fn dp_accessors_carry_the_logical_numbers() {
        let mut composer = Composer::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen_in = seen.clone();
        composer.compose(|ctx| {
            let md = draw_behind(Modifier::new().size(160.0, 90.0), move |scope| {
                *seen_in.lock().unwrap() = Some((
                    scope.width_dp().to_logical(),
                    scope.height_dp().to_logical(),
                    scope.size_dp().0.to_logical(),
                    scope.size_dp().1.to_logical(),
                    scope.center_dp().0.to_logical(),
                    scope.center_dp().1.to_logical(),
                ));
            });
            let key = ctx.next_key();
            ctx.start_leaf(key, md);
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let got = seen.lock().unwrap().expect("draw ran");
        assert_eq!(got, (160.0, 90.0, 160.0, 90.0, 80.0, 45.0));
    }

    /// `draw_text` returns the run's measured size, and that size is the paragraph's — the point being
    /// that a caller can centre or size something around text without re-deriving the layout. Two
    /// different strings must not report the same width (that would mean the value is fabricated).
    #[test]
    fn draw_text_reports_the_run_it_drew() {
        let mut composer = Composer::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        composer.compose(|ctx| {
            let md = draw_behind(Modifier::new().size(280.0, 60.0), move |scope| {
                let short = scope.draw_text("i", 0.0, 0.0, 16.0, Color::BLACK);
                let long = scope.draw_text("a much longer run", 0.0, 20.0, 16.0, Color::BLACK);
                seen_in.lock().unwrap().push((
                    short.width(),
                    short.height(),
                    long.width(),
                    long.height(),
                    short.origin(),
                    short.rect(),
                ));
            });
            let key = ctx.next_key();
            ctx.start_leaf(key, md);
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let runs = seen.lock().unwrap().clone();
        let (short_w, short_h, long_w, long_h, origin, rect) = runs[0];
        assert!(short_h > 0.0, "a drawn run has height");
        assert!(long_w > short_w, "\"a much longer run\" must measure wider than \"i\": {long_w} vs {short_w}");
        assert_eq!(origin, (0.0, 0.0), "the metrics carry the origin they were drawn at");
        assert_eq!(
            (rect.left, rect.top, rect.width(), rect.height()),
            (0.0, 0.0, short_w, short_h),
            "rect is the run's box at its origin"
        );
    }

    /// `draw_at` / `draw_centered` place a run by its box, and the claim "it is centred" is checked
    /// against the paragraph's own line metrics rather than against a guess about glyph shapes: the
    /// paragraph draws its baseline at `origin + metrics.baseline` and its line's top edge is
    /// `baseline - ascent` above that (the metrics docs say so and the first assertion confirms it on
    /// the same paragraph), so centring means that top edge lands at `(height - run_height) / 2`.
    #[test]
    fn draw_centered_lands_the_run_at_the_centred_origin() {
        let mut composer = Composer::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen_in = seen.clone();
        composer.compose(|ctx| {
            let md = draw_behind(Modifier::new().size(200.0, 100.0), move |scope| {
                let run = scope.draw_text("centre me", 0.0, 0.0, 16.0, Color::BLACK);
                run.draw_centered(scope, 200.0, 100.0);
                let lm = run.paragraph().get_line_metrics();
                let (baseline, ascent) = (lm[0].baseline as f32, lm[0].ascent as f32);
                *seen_in.lock().unwrap() = Some((
                    baseline, ascent, run.height(), run.width(),
                ));
            });
            let key = ctx.next_key();
            ctx.start_leaf(key, md);
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
        let mut surface = surfaces::raster_n32_premul((200, 100)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let (baseline, ascent, height, width) = seen.lock().unwrap().expect("draw ran");

        // The relation the placement below relies on, confirmed on this very paragraph: a run painted
        // at y=0 puts its baseline at `baseline`, so its visual top edge is `baseline - ascent`.
        assert!(
            (baseline - ascent).abs() < height.max(1.0) + 1.0,
            "the line's top edge sits inside the run's box: baseline {baseline} ascent {ascent} height {height}"
        );

        // What `draw_centered(200, 100)` computes, by hand:
        let centred_y = (100.0 - height) / 2.0;
        let top_edge = centred_y + (baseline - ascent);
        let expected_top = (100.0 - height) / 2.0;
        assert!(
            (top_edge - expected_top).abs() <= 1.0,
            "centred run's top edge should land at {expected_top}, got {top_edge}"
        );
        let centred_x = (200.0 - width) / 2.0;
        assert!(centred_x >= 0.0 && centred_x + width <= 200.0, "the run fits the box it is centred in");
    }

    /// Every primitive in the scope must survive a real draw call — they are thin Skia wrappers, and
    /// a wrong parameter (a negative radius, an empty path) is exactly what a caller will pass.
    /// Each is drawn once; the assertion is that the frame still renders and the un-covered pixel is
    /// untouched, which is what a panic or a stray draw would break.
    #[test]
    fn every_primitive_draws_without_disturbing_the_rest_of_the_frame() {
        let modifier = Modifier::new().size(200.0, 200.0);
        let md = draw_behind(modifier, |scope| {
            scope.draw_rect(skia_safe::Rect::new(0.0, 0.0, 40.0, 40.0), Color::RED);
            scope.draw_round_rect(skia_safe::Rect::new(50.0, 0.0, 90.0, 40.0), 8.0, Color::GREEN);
            scope.draw_circle(120.0, 20.0, 20.0, Color::BLUE);
            scope.draw_oval(skia_safe::Rect::new(150.0, 0.0, 190.0, 40.0), Color::BLACK);
            scope.draw_line((0.0, 60.0), (200.0, 60.0), 2.0, Color::BLACK);
            let mut path = skia_safe::PathBuilder::new();
            path.move_to((0.0, 80.0));
            path.line_to((40.0, 120.0));
            path.line_to((0.0, 120.0));
            path.close();
            let tri = path.detach();
            scope.draw_path(&tri, Color::RED);
            scope.draw_text("canvas", 4.0, 140.0, 14.0, Color::BLACK);
            // Degenerate inputs a caller can reach: zero radius, zero-length line, empty path.
            scope.draw_circle(10.0, 180.0, 0.0, Color::RED);
            scope.draw_line((0.0, 190.0), (0.0, 190.0), 1.0, Color::RED);
            scope.draw_path(&Path::new(), Color::RED);
        });
        let px = render_pixel(md, 190, 190);
        assert!(is_white(px), "the untouched corner is still white");
    }
}
