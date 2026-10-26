use super::*;

// ── LayoutDirection ──

/// The layout direction — it decides what Start/End mean for Row and the other horizontal layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    /// Left to right (Latin scripts).
    Ltr,
    /// Right to left (Arabic, Hebrew, and others).
    Rtl,
}

// ── Placement ──

/// Where a child ended up in its parent: the measure phase produces the size, the layout phase the
/// position.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// The size given to the child.
    pub size: Size,
    /// The child's position inside its parent.
    pub position: Offset,
}

// ── Arrangement ──

/// Main-axis arrangement (Compose's Arrangement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

// ── Alignment ──

/// Cross-axis alignment (Compose's `Alignment.Horizontal` / `Alignment.Vertical`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    End,
    Center,
    Stretch,
}

// ── ContentAlignment ──

/// Where a child sits inside a box, on BOTH axes — Compose's 2-D `Alignment`, the parameter of
/// `Box(contentAlignment = …)`, `AnimatedContent(contentAlignment = …)` and
/// `Modifier.animateContentSize(alignment = …)`.
///
/// The one-axis [`Alignment`] stays what it is: Column and Row align a child on their CROSS axis, and
/// that is Compose's `Alignment.Horizontal` / `Alignment.Vertical`. A box needs both, and winia had
/// only the diagonal values (`alignment(Alignment)` puts the same value on each axis, so `Start` means
/// `TopStart` and `End` means `BottomEnd`) — the mixed corners Compose allows were not expressible.
/// `Stretch` is winia's own extra on either axis, and stretches that axis to the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentAlignment {
    pub horizontal: Alignment,
    pub vertical: Alignment,
}

impl ContentAlignment {
    pub const TOP_START: Self = Self { horizontal: Alignment::Start, vertical: Alignment::Start };
    pub const TOP_CENTER: Self = Self { horizontal: Alignment::Center, vertical: Alignment::Start };
    pub const TOP_END: Self = Self { horizontal: Alignment::End, vertical: Alignment::Start };
    pub const CENTER_START: Self = Self { horizontal: Alignment::Start, vertical: Alignment::Center };
    pub const CENTER: Self = Self { horizontal: Alignment::Center, vertical: Alignment::Center };
    pub const CENTER_END: Self = Self { horizontal: Alignment::End, vertical: Alignment::Center };
    pub const BOTTOM_START: Self = Self { horizontal: Alignment::Start, vertical: Alignment::End };
    pub const BOTTOM_CENTER: Self = Self { horizontal: Alignment::Center, vertical: Alignment::End };
    pub const BOTTOM_END: Self = Self { horizontal: Alignment::End, vertical: Alignment::End };
    /// Both axes stretched to the box — winia's addition, since Compose stretches with
    /// `fillMaxSize()` instead.
    pub const STRETCH: Self = Self { horizontal: Alignment::Stretch, vertical: Alignment::Stretch };

    pub fn new(horizontal: Alignment, vertical: Alignment) -> Self {
        Self { horizontal, vertical }
    }

    /// The same value on both axes, which is what the one-axis builder means.
    pub fn both(a: Alignment) -> Self {
        Self { horizontal: a, vertical: a }
    }

    /// Compose's `Alignment.align(size, space, layoutDirection)`: the offset for a child of `child`
    /// inside a box of `space`, with the horizontal half mirroring under RTL.
    ///
    /// The mirroring is Compose's, not an extra: `Alignment.TopStart` is `BiasAlignment(-1f, -1f)`
    /// and the horizontal bias is NEGATED under RTL, so `Start` is the left edge in LTR and the right
    /// one in RTL (`Alignment.kt:114-121`). The vertical half never mirrors. Compose's
    /// `AbsoluteAlignment` (`TopLeft`, `CenterRight`, …) is the way to opt out of that; winia has no
    /// equivalent yet, which the module doc records.
    pub fn anchor(&self, child: Size, space: Size, direction: LayoutDirection) -> (f32, f32) {
        let horizontal = if direction == LayoutDirection::Rtl {
            match self.horizontal {
                Alignment::Start => Alignment::End,
                Alignment::End => Alignment::Start,
                other => other,
            }
        } else {
            self.horizontal
        };
        let axis = |a: Alignment, child: f32, space: f32| match a {
            Alignment::Start => 0.0,
            Alignment::End => space - child,
            Alignment::Center => (space - child) / 2.0,
            Alignment::Stretch => 0.0,
        };
        (
            axis(horizontal, child.width, space.width),
            axis(self.vertical, child.height, space.height),
        )
    }

    /// The size the child gets: its own, except on an axis that stretches to the box.
    pub fn child_size(&self, child: Size, space: Size) -> Size {
        Size::new(
            if self.horizontal == Alignment::Stretch { space.width } else { child.width },
            if self.vertical == Alignment::Stretch { space.height } else { child.height },
        )
    }
}

/// Recorded, not implemented: Compose also has `AbsoluteAlignment` (`TopLeft`, `CenterRight`, …), the
/// values that do NOT mirror under RTL, for callers that mean "the left edge, whatever the direction".
/// winia's equivalent is per-site (`Modifier::absolute_offset`), and nothing here needs the family
/// yet — the two components that relied on a non-mirroring box alignment now name a position that is
/// direction-symmetric instead (`NavigationDrawer` uses `Start` and lets the mirroring put it on the
/// trailing edge; `BottomSheetScaffold` uses `TOP_CENTER`, whose centring is symmetric).
///
/// The `Start`-on-both-axes alignment, Compose's default everywhere a 2-D one is taken.
impl Default for ContentAlignment {
    fn default() -> Self {
        Self::TOP_START
    }
}

/// A node's placed box in window coordinates — what `Modifier::on_globally_positioned` /
/// `on_placed` hand their callback.
///
/// The origin is the window's top-left in logical pixels and includes the scroll offsets of every
/// scrolling ancestor (the renderer translates them at paint time; the callback reports what the user
/// sees). Compose's equivalent is `LayoutCoordinates`; winia exposes the part a callback can use today
/// (position + size), not the coordinate-tree navigation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalPosition {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl GlobalPosition {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Top-left corner, in window coordinates.
    pub fn position(&self) -> Offset {
        Offset::new(self.x, self.y)
    }

    /// The placed size.
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// `(x, y, width, height)`, the shape the layout code keeps internally.
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.width, self.height)
    }
}
