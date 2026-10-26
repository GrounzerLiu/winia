//! The layout constraint model — Jetpack Compose's Constraints.
//!
//! Constraints describe the space a parent offers its child: the child must stay inside them
//! but is free to choose any size within the range.

/// Layout constraints: the space a parent offers its child.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Constraints {
/// An unbounded constraint (the child decides its own size).
    pub const UNBOUNDED: Constraints = Constraints {
        min_width: 0.0,
        max_width: f32::INFINITY,
        min_height: 0.0,
        max_height: f32::INFINITY,
    };

/// A constraint of a fixed size.
    pub fn fixed(width: f32, height: f32) -> Self {
        Constraints {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
        }
    }

/// A constraint over a range.
    pub fn new(min_width: f32, max_width: f32, min_height: f32, max_height: f32) -> Self {
        Constraints {
            min_width,
            max_width,
            min_height,
            max_height,
        }
    }

/// Clamps a size value into the legal range.
    pub fn constrain_width(&self, width: f32) -> f32 {
        width.clamp(self.min_width, self.max_width)
    }

    pub fn constrain_height(&self, height: f32) -> f32 {
        height.clamp(self.min_height, self.max_height)
    }

/// Whether the width is exact (min == max).
    pub fn has_fixed_width(&self) -> bool {
        self.min_width == self.max_width
    }

/// Whether the height is exact (min == max).
    pub fn has_fixed_height(&self) -> bool {
        self.min_height == self.max_height
    }

/// Loosens the constraint: min becomes 0, max is kept.
    /// Loosen only the height — Compose's `constraints.copy(minHeight = 0)`, which is what a container
    /// measuring a small overlay does when it wants the overlay's own height but still respects the width
    /// its parent demanded (`material3/Badge.kt:86-88`).
    pub fn loosen_height(&self) -> Constraints {
        Constraints {
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: 0.0,
            max_height: self.max_height,
        }
    }

    pub fn loosen(&self) -> Constraints {
        Constraints {
            min_width: 0.0,
            max_width: self.max_width,
            min_height: 0.0,
            max_height: self.max_height,
        }
    }

/// Tightens the width.
    pub fn tighten_width(&self, width: f32) -> Constraints {
        Constraints {
            min_width: width,
            max_width: width,
            min_height: self.min_height,
            max_height: self.max_height,
        }
    }

/// Tightens the height.
    pub fn tighten_height(&self, height: f32) -> Constraints {
        Constraints {
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: height,
            max_height: height,
        }
    }

/// Deflates by the space already used (for measuring the remaining children).
    pub fn offset(&self, dx: f32, dy: f32) -> Constraints {
        Constraints {
            min_width: (self.min_width - dx).max(0.0),
            max_width: (self.max_width - dx).max(0.0),
            min_height: (self.min_height - dy).max(0.0),
            max_height: (self.max_height - dy).max(0.0),
        }
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Constraints::UNBOUNDED
    }
}
