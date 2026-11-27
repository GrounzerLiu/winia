use crate::layout::LayoutDirection;
use crate::overlay::PopupPosition;
use winit::dpi::{LogicalPosition, LogicalSize, Position, Size};
use winit::window::{WindowAnchor, WindowConstraintAdjustment, WindowGravity, WindowPositioner};

/// A platform-neutral native popup placement plan.
///
/// `origin` is the final top-left in parent-local logical coordinates, including `offset`.
/// `anchor_rect` is also parent-local. `parent_size` is retained so window-aligned positions,
/// especially `Center`, can be expressed as a winit positioner without inventing an anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PopupPlacement {
    pub(crate) origin: (f32, f32),
    pub(crate) anchor_rect: Option<(f32, f32, f32, f32)>,
    pub(crate) content_size: (f32, f32),
    pub(crate) parent_size: (f32, f32),
    pub(crate) position: PopupPosition,
    pub(crate) direction: LayoutDirection,
    pub(crate) offset: (f32, f32),
    pub(crate) fit_around_anchor: bool,
    pub(crate) match_anchor_width: bool,
}

impl PopupPlacement {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        origin: (f32, f32),
        anchor_rect: Option<(f32, f32, f32, f32)>,
        content_size: (f32, f32),
        parent_size: (f32, f32),
        position: PopupPosition,
        direction: LayoutDirection,
        offset: (f32, f32),
        fit_around_anchor: bool,
        match_anchor_width: bool,
    ) -> Self {
        Self {
            origin,
            anchor_rect,
            content_size,
            parent_size,
            position,
            direction,
            offset,
            fit_around_anchor,
            match_anchor_width,
        }
    }

    /// Convert this plan into winit's parent-relative popup positioner.
    ///
    /// The in-window host remains the source of truth for the final `origin`. This conversion keeps
    /// the same requested edge and direction so Wayland can use its native positioner, while
    /// Windows and macOS can simulate the same anchor/gravity relationship.
    pub(crate) fn to_winit_positioner(&self) -> WindowPositioner {
        let window_aligned = self.anchor_rect.is_none() || self.position == PopupPosition::Center;
        let rect = if window_aligned {
            (0.0, 0.0, self.parent_size.0, self.parent_size.1)
        } else {
            self.anchor_rect.expect("native popup has an anchor")
        };
        let (anchor, gravity) = if window_aligned {
            self.window_aligned_anchor_and_gravity()
        } else {
            self.anchored_anchor_and_gravity()
        };
        let adjustment = if self.fit_around_anchor {
            WindowConstraintAdjustment::SLIDE_X
                | WindowConstraintAdjustment::SLIDE_Y
                | WindowConstraintAdjustment::FLIP_X
                | WindowConstraintAdjustment::FLIP_Y
        } else {
            WindowConstraintAdjustment::empty()
        };
        WindowPositioner::new(
            anchor,
            (
                Position::Logical(LogicalPosition::new(rect.0 as f64, rect.1 as f64)),
                Size::Logical(LogicalSize::new(rect.2 as f64, rect.3 as f64)),
            ),
            Position::Logical(LogicalPosition::new(
                self.offset.0 as f64,
                self.offset.1 as f64,
            )),
            gravity,
            adjustment,
        )
    }

    /// Resolve the final parent-local origin with winia's placement rules.
    pub(crate) fn to_parent_origin(&self) -> (f32, f32) {
        crate::app::overlay_host::resolve_overlay_placement(
            self.position,
            self.direction,
            self.anchor_rect,
            self.content_size,
            self.parent_size,
            self.offset,
            self.fit_around_anchor,
            None,
        )
        .origin
    }

    /// Anchor an external popup to the requested edge of the anchor rectangle.
    fn anchored_anchor_and_gravity(&self) -> (WindowAnchor, WindowGravity) {
        let rtl = self.direction == LayoutDirection::Rtl;
        match self.position {
            PopupPosition::TopLeft => (WindowAnchor::TopLeft, WindowGravity::TopRight),
            PopupPosition::TopCenter => (WindowAnchor::Top, WindowGravity::Top),
            PopupPosition::TopRight => (WindowAnchor::TopRight, WindowGravity::TopLeft),
            PopupPosition::Center => (WindowAnchor::Center, WindowGravity::Center),
            PopupPosition::BottomLeft => (WindowAnchor::BottomLeft, WindowGravity::BottomRight),
            PopupPosition::BottomCenter => (WindowAnchor::Bottom, WindowGravity::Bottom),
            PopupPosition::BottomRight => (WindowAnchor::BottomRight, WindowGravity::BottomLeft),
            PopupPosition::TopStart => {
                if rtl {
                    (WindowAnchor::TopRight, WindowGravity::TopLeft)
                } else {
                    (WindowAnchor::TopLeft, WindowGravity::TopRight)
                }
            }
            PopupPosition::TopEnd => {
                if rtl {
                    (WindowAnchor::TopLeft, WindowGravity::TopRight)
                } else {
                    (WindowAnchor::TopRight, WindowGravity::TopLeft)
                }
            }
            PopupPosition::BottomStart => {
                if rtl {
                    (WindowAnchor::BottomRight, WindowGravity::BottomLeft)
                } else {
                    (WindowAnchor::BottomLeft, WindowGravity::BottomRight)
                }
            }
            PopupPosition::BottomEnd => {
                if rtl {
                    (WindowAnchor::BottomLeft, WindowGravity::BottomRight)
                } else {
                    (WindowAnchor::BottomRight, WindowGravity::BottomLeft)
                }
            }
        }
    }

    /// Place a window-aligned popup inside the parent rectangle.
    fn window_aligned_anchor_and_gravity(&self) -> (WindowAnchor, WindowGravity) {
        let rtl = self.direction == LayoutDirection::Rtl;
        match self.position {
            PopupPosition::TopLeft => (WindowAnchor::TopLeft, WindowGravity::BottomRight),
            PopupPosition::TopCenter => (WindowAnchor::Top, WindowGravity::Bottom),
            PopupPosition::TopRight => (WindowAnchor::TopRight, WindowGravity::BottomLeft),
            PopupPosition::Center => (WindowAnchor::Center, WindowGravity::Center),
            PopupPosition::BottomLeft => (WindowAnchor::BottomLeft, WindowGravity::TopRight),
            PopupPosition::BottomCenter => (WindowAnchor::Bottom, WindowGravity::Top),
            PopupPosition::BottomRight => (WindowAnchor::BottomRight, WindowGravity::TopLeft),
            PopupPosition::TopStart => {
                if rtl {
                    (WindowAnchor::TopRight, WindowGravity::BottomLeft)
                } else {
                    (WindowAnchor::TopLeft, WindowGravity::BottomRight)
                }
            }
            PopupPosition::TopEnd => {
                if rtl {
                    (WindowAnchor::TopLeft, WindowGravity::BottomRight)
                } else {
                    (WindowAnchor::TopRight, WindowGravity::BottomLeft)
                }
            }
            PopupPosition::BottomStart => {
                if rtl {
                    (WindowAnchor::BottomRight, WindowGravity::TopLeft)
                } else {
                    (WindowAnchor::BottomLeft, WindowGravity::TopRight)
                }
            }
            PopupPosition::BottomEnd => {
                if rtl {
                    (WindowAnchor::BottomLeft, WindowGravity::TopRight)
                } else {
                    (WindowAnchor::BottomRight, WindowGravity::TopLeft)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(
        position: PopupPosition,
        direction: LayoutDirection,
        anchor_rect: Option<(f32, f32, f32, f32)>,
        offset: (f32, f32),
        fit_around_anchor: bool,
    ) -> PopupPlacement {
        PopupPlacement::new(
            (0.0, 0.0),
            anchor_rect,
            (60.0, 20.0),
            (400.0, 300.0),
            position,
            direction,
            offset,
            fit_around_anchor,
            false,
        )
    }

    #[test]
    fn rtl_start_and_end_use_the_matching_anchor_edges() {
        let anchor = Some((100.0, 100.0, 80.0, 40.0));
        let bottom_start = placement(
            PopupPosition::BottomStart,
            LayoutDirection::Rtl,
            anchor,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(bottom_start.anchor, WindowAnchor::BottomRight);
        assert_eq!(bottom_start.gravity, WindowGravity::BottomLeft);

        let bottom_end = placement(
            PopupPosition::BottomEnd,
            LayoutDirection::Rtl,
            anchor,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(bottom_end.anchor, WindowAnchor::BottomLeft);
        assert_eq!(bottom_end.gravity, WindowGravity::BottomRight);

        let top_start = placement(
            PopupPosition::TopStart,
            LayoutDirection::Rtl,
            anchor,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(top_start.anchor, WindowAnchor::TopRight);
        assert_eq!(top_start.gravity, WindowGravity::TopLeft);
    }

    #[test]
    fn absolute_positions_and_center_keep_their_geometry() {
        let anchor = Some((100.0, 100.0, 80.0, 40.0));
        let bottom_left = placement(
            PopupPosition::BottomLeft,
            LayoutDirection::Ltr,
            anchor,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(bottom_left.anchor, WindowAnchor::BottomLeft);
        assert_eq!(bottom_left.gravity, WindowGravity::BottomRight);

        let top_right = placement(
            PopupPosition::TopRight,
            LayoutDirection::Ltr,
            anchor,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(top_right.anchor, WindowAnchor::TopRight);
        assert_eq!(top_right.gravity, WindowGravity::TopLeft);

        let center = placement(
            PopupPosition::Center,
            LayoutDirection::Rtl,
            anchor,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(center.anchor, WindowAnchor::Center);
        assert_eq!(center.gravity, WindowGravity::Center);
        assert_eq!(
            center.anchor_rect.0,
            Position::Logical(LogicalPosition::new(0.0, 0.0))
        );
        assert_eq!(
            center.anchor_rect.1,
            Size::Logical(LogicalSize::new(400.0, 300.0))
        );
    }

    #[test]
    fn window_aligned_positions_use_the_parent_rect() {
        let positioner = placement(
            PopupPosition::BottomRight,
            LayoutDirection::Ltr,
            None,
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(positioner.anchor, WindowAnchor::BottomRight);
        assert_eq!(positioner.gravity, WindowGravity::TopLeft);
        assert_eq!(
            positioner.anchor_rect.1,
            Size::Logical(LogicalSize::new(400.0, 300.0))
        );
    }

    fn anchor_fraction(anchor: WindowAnchor) -> (f64, f64) {
        match anchor {
            WindowAnchor::Center => (0.5, 0.5),
            WindowAnchor::Top => (0.5, 0.0),
            WindowAnchor::Bottom => (0.5, 1.0),
            WindowAnchor::Left => (0.0, 0.5),
            WindowAnchor::Right => (1.0, 0.5),
            WindowAnchor::TopLeft => (0.0, 0.0),
            WindowAnchor::BottomLeft => (0.0, 1.0),
            WindowAnchor::TopRight => (1.0, 0.0),
            WindowAnchor::BottomRight => (1.0, 1.0),
            _ => (0.5, 0.5),
        }
    }

    fn gravity_fraction(gravity: WindowGravity) -> (f64, f64) {
        match gravity {
            WindowGravity::Center => (-0.5, -0.5),
            WindowGravity::Top => (-0.5, -1.0),
            WindowGravity::Bottom => (-0.5, 0.0),
            WindowGravity::Left => (-1.0, -0.5),
            WindowGravity::Right => (0.0, -0.5),
            WindowGravity::TopLeft => (-1.0, -1.0),
            WindowGravity::BottomLeft => (-1.0, 0.0),
            WindowGravity::TopRight => (0.0, -1.0),
            WindowGravity::BottomRight => (0.0, 0.0),
            _ => (-0.5, -0.5),
        }
    }

    fn unresolved_origin(
        positioner: &WindowPositioner,
        popup_size: LogicalSize<f64>,
    ) -> (f64, f64) {
        let anchor_position = positioner.anchor_rect.0.to_logical::<f64>(1.0);
        let anchor_size = positioner.anchor_rect.1.to_logical::<f64>(1.0);
        let offset = positioner.offset.to_logical::<f64>(1.0);
        let (anchor_x, anchor_y) = anchor_fraction(positioner.anchor);
        let (gravity_x, gravity_y) = gravity_fraction(positioner.gravity);
        (
            anchor_position.x
                + anchor_size.width * anchor_x
                + popup_size.width * gravity_x
                + offset.x,
            anchor_position.y
                + anchor_size.height * anchor_y
                + popup_size.height * gravity_y
                + offset.y,
        )
    }

    fn assert_origin(actual: (f64, f64), expected: (f32, f32)) {
        assert!((actual.0 - expected.0 as f64).abs() < 1e-9);
        assert!((actual.1 - expected.1 as f64).abs() < 1e-9);
    }

    #[test]
    fn anchored_positioner_origins_match_winia_for_every_position_and_direction() {
        let anchor = Some((100.0, 100.0, 80.0, 40.0));
        let popup_size = LogicalSize::new(60.0, 20.0);
        let offset = (7.0, -3.0);
        for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
            for position in [
                PopupPosition::TopLeft,
                PopupPosition::TopCenter,
                PopupPosition::TopRight,
                PopupPosition::Center,
                PopupPosition::BottomLeft,
                PopupPosition::BottomCenter,
                PopupPosition::BottomRight,
                PopupPosition::TopStart,
                PopupPosition::TopEnd,
                PopupPosition::BottomStart,
                PopupPosition::BottomEnd,
            ] {
                let positioner =
                    placement(position, direction, anchor, offset, false).to_winit_positioner();
                let expected = crate::overlay::resolve_popup_position(
                    position,
                    direction,
                    anchor,
                    (60.0, 20.0),
                    (400.0, 300.0),
                );
                assert_origin(
                    unresolved_origin(&positioner, popup_size),
                    (expected.0 + offset.0, expected.1 + offset.1),
                );
            }
        }
    }

    #[test]
    fn window_aligned_positioner_origins_match_winia_for_every_position_and_direction() {
        let popup_size = LogicalSize::new(60.0, 20.0);
        let offset = (7.0, -3.0);
        for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
            for position in [
                PopupPosition::TopLeft,
                PopupPosition::TopCenter,
                PopupPosition::TopRight,
                PopupPosition::Center,
                PopupPosition::BottomLeft,
                PopupPosition::BottomCenter,
                PopupPosition::BottomRight,
                PopupPosition::TopStart,
                PopupPosition::TopEnd,
                PopupPosition::BottomStart,
                PopupPosition::BottomEnd,
            ] {
                let positioner =
                    placement(position, direction, None, offset, false).to_winit_positioner();
                let expected = crate::overlay::resolve_popup_position(
                    position,
                    direction,
                    None,
                    (60.0, 20.0),
                    (400.0, 300.0),
                );
                assert_origin(
                    unresolved_origin(&positioner, popup_size),
                    (expected.0 + offset.0, expected.1 + offset.1),
                );
            }
        }
    }

    #[test]
    fn offset_is_transferred_to_the_positioner() {
        let positioner = placement(
            PopupPosition::BottomLeft,
            LayoutDirection::Ltr,
            Some((100.0, 100.0, 80.0, 40.0)),
            (7.0, -3.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(
            positioner.offset,
            Position::Logical(LogicalPosition::new(7.0, -3.0))
        );
    }

    #[test]
    fn fit_around_anchor_enables_slide_and_flip_without_resize() {
        let fitted = placement(
            PopupPosition::BottomLeft,
            LayoutDirection::Ltr,
            Some((100.0, 100.0, 80.0, 40.0)),
            (0.0, 0.0),
            true,
        )
        .to_winit_positioner();
        assert!(
            fitted
                .constraint_adjustment
                .contains(WindowConstraintAdjustment::SLIDE_X)
        );
        assert!(
            fitted
                .constraint_adjustment
                .contains(WindowConstraintAdjustment::SLIDE_Y)
        );
        assert!(
            fitted
                .constraint_adjustment
                .contains(WindowConstraintAdjustment::FLIP_X)
        );
        assert!(
            fitted
                .constraint_adjustment
                .contains(WindowConstraintAdjustment::FLIP_Y)
        );
        assert!(
            !fitted
                .constraint_adjustment
                .contains(WindowConstraintAdjustment::RESIZE_X)
        );
        assert!(
            !fitted
                .constraint_adjustment
                .contains(WindowConstraintAdjustment::RESIZE_Y)
        );

        let plain = placement(
            PopupPosition::BottomLeft,
            LayoutDirection::Ltr,
            Some((100.0, 100.0, 80.0, 40.0)),
            (0.0, 0.0),
            false,
        )
        .to_winit_positioner();
        assert_eq!(
            plain.constraint_adjustment,
            WindowConstraintAdjustment::empty()
        );
    }
}
