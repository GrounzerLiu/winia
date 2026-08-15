use crate::offset::Offset;
use crate::point::Point;

/**
 * An immutable, 2D, axis-aligned, floating-point rectangle whose coordinates are relative to a
 * given origin.
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[inline]
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    #[inline]
    pub fn size(&self) -> (f32, f32) {
        (self.width(), self.height())
    }

    #[inline]
    pub fn is_infinite(&self) -> bool {
        self.left.is_infinite()
            || self.top.is_infinite()
            || self.right.is_infinite()
            || self.bottom.is_infinite()
    }

    #[inline]
    pub fn is_finite(&self) -> bool {
        self.left.is_finite()
            && self.top.is_finite()
            && self.right.is_finite()
            && self.bottom.is_finite()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    #[inline]
    pub fn translate(&self, offset: Offset) -> Rect {
        Rect {
            left: self.left + offset.x(),
            top: self.top + offset.y(),
            right: self.right + offset.x(),
            bottom: self.bottom + offset.y(),
        }
    }

    #[inline]
    pub fn translate_xy(&self, dx: f32, dy: f32) -> Rect {
        Rect {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    #[inline]
    pub fn inflate(&self, delta: f32) -> Rect {
        Rect {
            left: self.left - delta,
            top: self.top - delta,
            right: self.right + delta,
            bottom: self.bottom + delta,
        }
    }

    #[inline]
    pub fn deflate(&self, delta: f32) -> Rect {
        self.inflate(-delta)
    }

    #[inline]
    pub fn intersect(&self, other: &Rect) -> Rect {
        Rect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    #[inline]
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    #[inline]
    pub fn min_dimension(&self) -> f32 {
        self.width().abs().min(self.height().abs())
    }

    #[inline]
    pub fn max_dimension(&self) -> f32 {
        self.width().abs().max(self.height().abs())
    }

    #[inline]
    pub fn top_left(&self) -> Offset {
        Point(self.left, self.top)
    }

    #[inline]
    pub fn top_center(&self) -> Offset {
        Point(self.left + self.width() * 0.5, self.top)
    }

    #[inline]
    pub fn top_right(&self) -> Offset {
        Point(self.right, self.top)
    }

    #[inline]
    pub fn center_left(&self) -> Offset {
        Point(self.left, self.top + self.height() * 0.5)
    }

    #[inline]
    pub fn center(&self) -> Offset {
        Point(self.left + self.width() * 0.5, self.top + self.height() * 0.5)
    }

    #[inline]
    pub fn center_right(&self) -> Offset {
        Point(self.right, self.top + self.height() * 0.5)
    }

    #[inline]
    pub fn bottom_left(&self) -> Offset {
        Point(self.left, self.bottom)
    }

    #[inline]
    pub fn bottom_center(&self) -> Offset {
        Point(self.left + self.width() * 0.5, self.bottom)
    }

    #[inline]
    pub fn bottom_right(&self) -> Offset {
        Point(self.right, self.bottom)
    }

    pub fn contains(&self, point: Offset) -> bool {
        point.x() >= self.left &&
        point.x() <= self.right &&
        point.y() >= self.top &&
        point.y() <= self.bottom
    }
}