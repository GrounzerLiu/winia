//! `Shape` — how a background, border or clip is outlined (Compose's `graphics.Shape`).


/// 形状描述（用于 background / border / clip）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// 矩形（可带圆角）
    RoundedRect { corner_radius: f32 },
    /// 仅顶部圆角（对齐 M3 BottomSheet 顶部 28dp——底部直角贴屏）
    TopRoundedRect { radius: f32 },
    /// Rounded on the two RIGHT corners only (upper-right + lower-right), left edge
    /// square. Geometric rather than direction-resolved: a modal navigation drawer
    /// docks at the leading edge and rounds the side facing the content, so an LTR
    /// drawer picks this one and its RTL counterpart picks [`Shape::LeftRoundedRect`].
    /// (Compose reaches the same pair through a single `CornerLargeEnd` token whose
    /// side follows the layout direction; a winia `Shape` carries no direction.)
    RightRoundedRect { radius: f32 },
    /// Mirror of [`Shape::RightRoundedRect`] — rounded on the two LEFT corners only.
    LeftRoundedRect { radius: f32 },
    /// 胶囊（圆角 = 短边一半——对标 Compose `CornerFull`，material3
    /// Button 默认形状；宽高变化时自动跟随）
    Pill,
    /// Percent-50 corner, i.e. Compose's `CircleShape` == `RoundedCornerShape(50)`: on a
    /// square box that is a circle, and on a NON-square box a stadium that fills the whole
    /// box (identical to `Pill`). A true inscribed circle was the old behaviour and was
    /// wrong against Compose.
    Circle,
    /// A rectangle with an independent radius per corner, Compose's
    /// `RoundedCornerShape(topStart, topEnd, bottomEnd, bottomStart)`. Geometric rather than
    /// direction-resolved, like [`Shape::RightRoundedRect`] and [`Shape::LeftRoundedRect`]: a
    /// caller that wants start/end semantics resolves the direction itself (material3's split
    /// button does; `SplitButtonDefaults` reads it from `WiniaTheme::direction`).
    ///
    /// The radii are pixels of *this* box, so a caller that needs Compose's `CornerFull` — a
    /// percent-50 corner, which is half the SHORT side — computes `height / 2` for a button that
    /// is wider than it is tall. Feeding a percent-shaped corner as a fixed radius keeps
    /// `Shape::Pill`'s behaviour only while that holds, which is why the split button derives it
    /// from its own container height.
    Corners {
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    },
    /// 直角矩形
    Rectangle,
    /// A `CornerFull` (percent-50) corner part-way to a FIXED radius: `t = 0.0` is exactly
    /// [`Shape::Pill`], `t = 1.0` exactly [`Shape::RoundedRect`] with `corner_radius`.
    ///
    /// This is what Compose animates when a shape morphs between a `PercentCornerSize(50)` and a
    /// fixed `CornerSize` — material3's `ButtonShapes(shape, pressedShape)`
    /// (`Button.kt:1922-1939`) and the icon buttons' press morph both do exactly that. winia's
    /// `Shape` had no way to express it: `Pill` and `Circle` carry no radius to interpolate, and
    /// a `RoundedRect` radius is fixed, so the percent-50 end had nothing to travel FROM.
    ///
    /// The corner is a percent of the SHORT side, so it can only be resolved once the box is
    /// known — see [`Shape::resolved_for_box`], which every paint, clip and shadow path calls
    /// first.
    PillToRounded {
        corner_radius: f32,
        t: f32,
    },
}

impl Shape {
    /// The shape a `width` x `height` box actually paints, with a percent-corner morph resolved
    /// to the fixed radius it is at *now*.
    ///
    /// Only [`Shape::PillToRounded`] is rewritten; every other variant is returned unchanged, so
    /// the paint, clip and shadow paths keep matching the arms they already had. A caller that
    /// skips this and matches on the raw shape would land on the `PillToRounded` arm, which is
    /// why the renderer calls it at the top of each resolution.
    pub fn resolved_for_box(&self, width: f32, height: f32) -> Shape {
        match *self {
            Shape::PillToRounded { corner_radius, t } => {
                let percent = width.min(height) / 2.0;
                Shape::rounded(percent + (corner_radius - percent) * t)
            }
            other => other,
        }
    }
}

impl Shape {
    pub fn rounded(corner_radius: f32) -> Self {
        Shape::RoundedRect { corner_radius }
    }

    pub fn top_rounded(radius: f32) -> Self {
        Shape::TopRoundedRect { radius }
    }

    /// Rounded on the two right corners (see [`Shape::RightRoundedRect`]).
    pub fn right_rounded(radius: f32) -> Self {
        Shape::RightRoundedRect { radius }
    }

    /// Rounded on the two left corners (see [`Shape::LeftRoundedRect`]).
    pub fn left_rounded(radius: f32) -> Self {
        Shape::LeftRoundedRect { radius }
    }

    /// 胶囊形状（对标 Compose `RoundedCornerShape(50)`——短边一半圆角）
    pub fn pill() -> Self {
        Shape::Pill
    }

    /// Per-corner radii, in the order Compose's `RoundedCornerShape` takes them
    /// (top-start, top-end, bottom-end, bottom-start) but in GEOMETRIC corners — see
    /// [`Shape::Corners`].
    pub fn corners(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Shape::Corners { top_left, top_right, bottom_right, bottom_left }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PillToRounded` resolves to a plain radius once the box is known: its two ends are
    /// exactly `Pill` and `RoundedRect`, and it travels linearly between them. The percent end is
    /// half the SHORT side, so a 100 x 40 box starts at 20, not at 50.
    #[test]
    fn a_pill_transition_resolves_against_the_box() {
        let at = |t: f32| Shape::PillToRounded { corner_radius: 8.0, t }.resolved_for_box(100.0, 40.0);

        assert_eq!(at(0.0), Shape::rounded(20.0), "t = 0 is exactly Shape::Pill");
        assert_eq!(at(1.0), Shape::rounded(8.0), "t = 1 is exactly the fixed radius");
        assert_eq!(at(0.5), Shape::rounded(14.0), "and it travels between them");

        // Half the SHORT side, not half the width: the same call in a tall box uses the width.
        assert_eq!(
            Shape::PillToRounded { corner_radius: 8.0, t: 0.0 }.resolved_for_box(40.0, 100.0),
            Shape::rounded(20.0)
        );
    }

    /// Every other variant passes through untouched — the resolution exists so the paint, clip and
    /// shadow paths keep matching the arms they already had.
    #[test]
    fn every_other_shape_passes_through_the_resolver() {
        for shape in [
            Shape::Rectangle,
            Shape::rounded(4.0),
            Shape::Pill,
            Shape::Circle,
            Shape::corners(1.0, 2.0, 3.0, 4.0),
            Shape::top_rounded(6.0),
        ] {
            assert_eq!(shape.resolved_for_box(80.0, 40.0), shape, "{shape:?} is unchanged");
        }
    }
}
