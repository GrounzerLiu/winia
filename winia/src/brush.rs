//! Brushes: how a shape is filled — Compose's `Brush`.
//!
//! ```ignore
//! Modifier::new().background_brush(
//!     Brush::linear_gradient([Color::RED, Color::BLUE]),
//!     Shape::rounded(12.0),
//! )
//! ```
//!
//! # Coordinates are FRACTIONS of the node's bounds
//!
//! This is the one deliberate difference from Compose, and the rule to remember:
//! [`Brush::from_to`] takes `(0.0, 0.0)`..`(1.0, 1.0)` across the node, `Brush::center` likewise, and
//! [`Brush::radius`] is a fraction of the node's **shorter** side. Compose instead takes absolute
//! `Offset`s (with `Offset.Infinite` meaning "the bounds").
//!
//! Fractions are what a reusable component needs: a card highlight written as "top-left to
//! bottom-right" fills any card, where an absolute offset would need the size and would break the
//! moment the node is measured differently. A caller that genuinely wants absolute pixels can divide by
//! the size it already knows — see the note on `Offset.Infinite` in `docs/brush.md`.
//!
//! # Fill, not stroke
//!
//! A brush fills a shape; the shapes come from the same [`crate::modifier::Shape`] set
//! `Modifier::background` uses, including the rounded and one-sided variants.
//!
//! # Animated brushes
//!
//! `background_brush` accepts a closure (`impl Fn() -> Brush`) as well as a value, mirroring
//! `background`'s handling of animated colors: read animated `State`s inside the closure and the
//! gradient moves with them.

use crate::modifier::Color;

/// How a gradient repeats outside its start/end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushTile {
    /// Extend the edge colors — the default, and what a plain gradient wants.
    #[default]
    Clamp,
    /// Tile the gradient again.
    Repeat,
    /// Tile it mirrored.
    Mirror,
}

/// What a gradient's `from` point means — the one thing the constructors need to know to pick a
/// sensible default for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GradientKind {
    /// `from` is where the run starts: the node's left-middle.
    Linear,
    /// `from` is the centre: the node's middle.
    Centered,
}

/// A one- or two-point gradient.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientBrush {
    /// Stop colors, in order along the gradient.
    colors: Vec<Color>,
    /// Explicit stop positions in `0.0..=1.0`, one per color, strictly increasing. `None` spreads the
    /// colors evenly — the common case, and the one Compose's bare `colors` list means.
    positions: Option<Vec<f32>>,
    /// Fractional start / centre, depending on the gradient kind.
    from: (f32, f32),
    /// Fractional end (linear only).
    to: (f32, f32),
    /// Radius as a fraction of the node's shorter side (radial only).
    radius: f32,
    tile: BrushTile,
}

/// How to fill a shape — Compose's `Brush`.
#[derive(Debug, Clone, PartialEq)]
pub enum Brush {
    /// A single color: the same result as `Modifier::background`, kept here so a caller can choose a
    /// brush at runtime without switching modifier methods.
    Solid(Color),
    /// Linear: color runs from [`GradientBrush::from_to`]'s start to its end.
    Linear(GradientBrush),
    /// Radial: color runs outward from the centre, reaching its last stop at `radius`.
    Radial(GradientBrush),
    /// Sweep: color runs around the centre, a full turn clockwise from 3 o'clock.
    Sweep(GradientBrush),
}

impl Brush {
    /// A single color.
    pub fn solid(color: Color) -> Self {
        Brush::Solid(color)
    }

    /// Colors running left to right across the node, spread evenly.
    pub fn linear_gradient(colors: impl Into<Vec<Color>>) -> Self {
        Brush::Linear(GradientBrush::new(colors.into(), GradientKind::Linear))
    }

    /// Colors running outward from the centre, spread evenly, reaching the last one at half the
    /// node's shorter side (so the gradient ends at the nearest pair of edges).
    pub fn radial_gradient(colors: impl Into<Vec<Color>>) -> Self {
        Brush::Radial(GradientBrush::new(colors.into(), GradientKind::Centered))
    }

    /// Colors running a full turn around the node's centre, spread evenly.
    ///
    /// The sweep starts at 3 o'clock and runs clockwise (Skia's convention, the same as CSS
    /// `conic-gradient`), and always covers the full turn — the `HueMethod`/angle range is not
    /// configurable, matching Compose's `Brush.sweepGradient`.
    pub fn sweep_gradient(colors: impl Into<Vec<Color>>) -> Self {
        Brush::Sweep(GradientBrush::new(colors.into(), GradientKind::Centered))
    }

    /// Linear: where the gradient starts and ends, as fractions of the node's bounds.
    ///
    /// `(0.0, 0.0)` is the top-left corner, `(1.0, 1.0)` the bottom-right.
    pub fn from_to(self, from: (f32, f32), to: (f32, f32)) -> Self {
        self.map_gradient(|g| GradientBrush {
            from,
            to,
            ..g
        })
    }

    /// Linear: a diagonal from the top-left corner to the bottom-right — the "card highlight" default.
    pub fn diagonal(self) -> Self {
        self.from_to((0.0, 0.0), (1.0, 1.0))
    }

    /// Linear: top to bottom.
    pub fn vertical(self) -> Self {
        self.from_to((0.5, 0.0), (0.5, 1.0))
    }

    /// Linear: left to right (the default for a linear brush built without `from_to`).
    pub fn horizontal(self) -> Self {
        self.from_to((0.0, 0.5), (1.0, 0.5))
    }

    /// Radial / sweep: the centre, as fractions of the node's bounds.
    pub fn center(self, x: f32, y: f32) -> Self {
        self.map_gradient(|g| GradientBrush {
            from: (x, y),
            ..g
        })
    }

    /// Radial: how far the last stop reaches, as a fraction of the node's SHORTER side.
    pub fn radius(self, radius: f32) -> Self {
        self.map_gradient(|g| GradientBrush { radius, ..g })
    }

    /// Where each color sits, one position per color in `0.0..=1.0` and strictly increasing. A brush
    /// built without this spreads its colors evenly.
    pub fn positions(self, positions: impl Into<Vec<f32>>) -> Self {
        let positions = positions.into();
        debug_assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "gradient stop positions must be strictly increasing, got {positions:?}"
        );
        self.map_gradient(|g| {
            debug_assert_eq!(
                positions.len(),
                g.colors.len(),
                "one position per color: {} positions for {} colors",
                positions.len(),
                g.colors.len()
            );
            GradientBrush {
                positions: Some(positions),
                ..g
            }
        })
    }

    /// How the gradient repeats outside its start/end. Defaults to
    /// [`BrushTile::Clamp`], which extends the edge colors.
    pub fn tile(self, tile: BrushTile) -> Self {
        self.map_gradient(|g| GradientBrush { tile, ..g })
    }

    /// The stops, for a caller that wants to read them back (a theme swap, a debug dump).
    pub fn colors(&self) -> &[Color] {
        match self {
            Brush::Solid(color) => std::slice::from_ref(color),
            Brush::Linear(g) | Brush::Radial(g) | Brush::Sweep(g) => &g.colors,
        }
    }

    /// Apply `f` to the gradient inside this brush; a solid brush is returned unchanged (there is
    /// nothing to configure — a gradient setting on a solid color is meaningless, not an error).
    fn map_gradient(self, f: impl FnOnce(GradientBrush) -> GradientBrush) -> Self {
        match self {
            Brush::Solid(_) => self,
            Brush::Linear(g) => Brush::Linear(f(g)),
            Brush::Radial(g) => Brush::Radial(f(g)),
            Brush::Sweep(g) => Brush::Sweep(f(g)),
        }
    }
}

impl GradientBrush {
    /// Evenly spread colors. `from` means different things per kind — the start of a linear run, or the
    /// CENTRE of a radial/sweep — so the default differs: a linear brush runs across the node, and the
    /// others sit in its middle.
    ///
    /// Getting this wrong is not subtle: a sweep whose centre defaulted to the linear start (the left
    /// edge's middle) paints its whole pattern around that point, which is nothing like a conic
    /// gradient. Measured, then fixed.
    fn new(colors: Vec<Color>, kind: GradientKind) -> Self {
        let from = match kind {
            GradientKind::Linear => (0.0, 0.5),
            GradientKind::Centered => (0.5, 0.5),
        };
        Self {
            colors,
            positions: None,
            from,
            to: (1.0, 0.5),
            radius: 0.5,
            tile: BrushTile::Clamp,
        }
    }

    pub(crate) fn stops(&self) -> &[Color] {
        &self.colors
    }

    pub(crate) fn positions_ref(&self) -> Option<&[f32]> {
        self.positions.as_deref()
    }

    pub(crate) fn from(&self) -> (f32, f32) {
        self.from
    }

    pub(crate) fn to(&self) -> (f32, f32) {
        self.to
    }

    pub(crate) fn radius_value(&self) -> f32 {
        self.radius
    }

    pub(crate) fn tile_mode(&self) -> BrushTile {
        self.tile
    }
}

// ── the modifier-facing source ──

/// What `Modifier::background_brush` accepts: a [`Brush`], or a closure returning one for a gradient
/// that follows animated state. Mirrors `BackgroundColor`'s handling of animated colors.
pub struct BrushSource(pub(crate) std::sync::Arc<dyn Fn() -> Brush + Send + Sync>);

impl From<Brush> for BrushSource {
    fn from(brush: Brush) -> Self {
        BrushSource(std::sync::Arc::new(move || brush.clone()))
    }
}

impl<F: Fn() -> Brush + Send + Sync + 'static> From<F> for BrushSource {
    fn from(f: F) -> Self {
        BrushSource(std::sync::Arc::new(f))
    }
}

impl std::fmt::Debug for BrushSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BrushSource(<dynamic>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Color {
        Color::from_argb(255, 255, 0, 0)
    }

    fn blue() -> Color {
        Color::from_argb(255, 0, 0, 255)
    }

    #[test]
    fn a_linear_gradient_defaults_to_left_to_right() {
        let brush = Brush::linear_gradient([red(), blue()]);
        let Brush::Linear(gradient) = &brush else {
            panic!("a linear constructor must build a linear brush");
        };
        assert_eq!(gradient.from(), (0.0, 0.5), "default start is the left edge's middle");
        assert_eq!(gradient.to(), (1.0, 0.5), "and default end is the right edge's");
        assert_eq!(brush.colors(), &[red(), blue()]);
        assert_eq!(gradient.positions_ref(), None, "no explicit stops means evenly spread");
        assert_eq!(gradient.tile_mode(), BrushTile::Clamp);
    }

    #[test]
    fn the_orientation_helpers_set_the_expected_endpoints() {
        let endpoints = |brush: Brush| {
            let Brush::Linear(gradient) = brush else { panic!("expected linear") };
            (gradient.from(), gradient.to())
        };
        assert_eq!(endpoints(Brush::linear_gradient([red()]).horizontal()), ((0.0, 0.5), (1.0, 0.5)));
        assert_eq!(endpoints(Brush::linear_gradient([red()]).vertical()), ((0.5, 0.0), (0.5, 1.0)));
        assert_eq!(endpoints(Brush::linear_gradient([red()]).diagonal()), ((0.0, 0.0), (1.0, 1.0)));
    }

    #[test]
    fn a_radial_gradient_centres_at_half_the_shorter_side() {
        let brush = Brush::radial_gradient([red(), blue()]).center(0.25, 0.75);
        let Brush::Radial(gradient) = &brush else {
            panic!("a radial constructor must build a radial brush");
        };
        assert_eq!(gradient.from(), (0.25, 0.75));
        // The default centre is the node's middle, not the linear brush's left edge — the bug the
        // pixel tests in render_snapshot.rs caught.
        let default = Brush::radial_gradient([red()]);
        let Brush::Radial(default) = &default else { panic!("expected radial") };
        assert_eq!(default.from(), (0.5, 0.5));
        assert_eq!(gradient.radius_value(), 0.5, "reaching the nearest pair of edges by default");
    }

    #[test]
    fn settings_on_a_solid_brush_are_ignored_rather_than_an_error() {
        // A caller that picks `Brush::Solid` at runtime still builds the same chain; a gradient
        // setting simply has nothing to configure.
        let brush = Brush::solid(red())
            .from_to((0.0, 0.0), (1.0, 1.0))
            .radius(0.25)
            .center(0.1, 0.2)
            .tile(BrushTile::Mirror);
        assert_eq!(brush, Brush::Solid(red()));
    }

    #[test]
    fn positions_and_tile_are_carried_through() {
        let brush = Brush::radial_gradient([red(), blue()])
            .positions([0.2, 0.9])
            .tile(BrushTile::Repeat);
        let Brush::Radial(gradient) = &brush else { panic!("expected radial") };
        assert_eq!(gradient.positions_ref(), Some(&[0.2, 0.9][..]));
        assert_eq!(gradient.tile_mode(), BrushTile::Repeat);
    }

    #[test]
    fn a_source_accepts_a_value_or_a_closure() {
        // Both forms have to coexist for `background_brush` to be one method.
        let from_value: BrushSource = Brush::solid(red()).into();
        assert_eq!((from_value.0)(), Brush::Solid(red()));

        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_in = flag.clone();
        let from_closure: BrushSource = (move || {
            if flag_in.load(std::sync::atomic::Ordering::Relaxed) {
                Brush::solid(blue())
            } else {
                Brush::solid(red())
            }
        })
        .into();
        assert_eq!((from_closure.0)(), Brush::Solid(red()), "the closure is re-read each time");
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!((from_closure.0)(), Brush::Solid(blue()));
    }
}
