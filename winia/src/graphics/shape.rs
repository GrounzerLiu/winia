//! `Shape` — how a background, border or clip is outlined (Compose's `graphics.Shape`).

use super::Color;

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
